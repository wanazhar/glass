use super::*;

#[tokio::test]
async fn native_content_process_script_exposes_web_idl_identity() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/identity"));
        let body = "<!doctype html><!--worker-root--><html><body><!--worker-body--><div style='height: 120px'></div><input id='name'></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 240,
                height: 100,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/identity")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let identity = engine
        .evaluate_async(
            "(() => { const input = document.getElementById('name'); const text = document.createTextNode('worker'); const rootComment = Array.from(document.childNodes).find(node => node instanceof Comment && node.data === 'worker-root'); const bodyComment = document.body.firstChild; const parsedDoctype = document.doctype; parsedDoctype.remove(); const createdDoctype = document.implementation.createDocumentType('worker-created'); document.appendChild(createdDoctype); globalThis.__persistentWorkerDoctype = createdDoctype; return { window: window instanceof Window, document: document instanceof Document, input: [input instanceof Node, input instanceof Element, input instanceof HTMLElement, input instanceof HTMLInputElement, input.nodeType === 1, input.nodeName === 'INPUT', input.localName === 'input', input.ownerDocument === document], nodes: document.querySelectorAll('input') instanceof NodeList, tags: document.getElementsByTagName('input') instanceof HTMLCollection, location: location instanceof Location, event: new Event('load') instanceof Event, text: [text instanceof Node, text instanceof CharacterData, text instanceof Text, text.data, text.nodeValue, text.length], domNodes: [document.doctype instanceof DocumentType, document.doctype.name, document.doctype === createdDoctype, createdDoctype.parentNode === document, rootComment instanceof Comment, rootComment.data, bodyComment instanceof Comment, bodyComment.parentNode === document.body, document.body.textContent], surfaces: [document.head === null, document.forms instanceof HTMLCollection, document.forms.length === 0, document.scripts instanceof HTMLCollection, document.scrollingElement === document.documentElement] }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        identity,
        serde_json::json!({
            "window": true,
            "document": true,
            "input": [true, true, true, true, true, true, true, true],
            "nodes": true,
            "tags": true,
            "location": true,
            "event": true,
            "text": [true, true, true, "worker", "worker", 6],
            "domNodes": [true, "worker-created", true, true, true, "worker-root", true, true, ""],
            "surfaces": [false, true, true, true, true],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[globalThis.__persistentWorkerDoctype === document.doctype, document.doctype.name, document.doctype.parentNode === document]",
            )
            .await
            .unwrap(),
        serde_json::json!([true, "worker-created", true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const before = document.title; document.title = 'Worker title'; return [before, document.title, document.head !== null, document.querySelector('title').textContent]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["", "Worker title", true, "Worker title"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[document.title, document.head.tagName, document.head.firstElementChild.tagName]"
            )
            .await
            .unwrap(),
        serde_json::json!(["Worker title", "HEAD", "TITLE"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const input = document.getElementById('name'); const clone = input.cloneNode(); return [clone !== input, clone.isEqualNode(input), clone.parentNode === null, clone.getRootNode() === clone, input.getRootNode() === document]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, true, true, true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const input = document.getElementById('name'); const detached = document.createElement('aside'); const root = input.getRootNode(); input.outerHTML = \"<textarea id='worker-replacement'>updated</textarea>\"; const replacement = document.getElementById('worker-replacement'); const result = [root === document, detached.getRootNode() === detached, replacement.tagName, replacement.textContent, replacement.parentElement === document.body, document.getElementById('name')]; replacement.outerHTML = \"<input id='name'>\"; return result; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, "TEXTAREA", "updated", true, null])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const host = document.createElement('article'); const child = document.createElement('span'); child.id = 'persistent-worker'; const text = document.createTextNode('persistent worker text'); const comment = document.createComment('persistent worker comment'); host.append(child, text, comment); document.body.appendChild(host); globalThis.__persistentWorkerHost = host; globalThis.__persistentWorkerChild = child; globalThis.__persistentWorkerText = text; globalThis.__persistentWorkerComment = comment; return [child.isConnected, child.parentNode === host, text.parentNode === host, comment instanceof Comment, comment.parentNode === host, comment.data]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, true, true, true, "persistent worker comment"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const host = globalThis.__persistentWorkerHost; const child = globalThis.__persistentWorkerChild; const text = globalThis.__persistentWorkerText; const comment = globalThis.__persistentWorkerComment; const current = document.getElementById('persistent-worker'); const currentComment = host.lastChild; const result = [current === child, host.contains(child), child.getRootNode() === document, child.parentElement === host, currentComment === comment, host.contains(text), text.parentNode === host, text.getRootNode() === document, text.data, comment.data, comment.parentNode === host, comment instanceof Comment]; text.data = 'updated'; host.remove(); return result; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, true, true, true, true, true, true, "persistent worker text", "persistent worker comment", true, true])
    );
    let before_scroll = engine
        .evaluate_async(
            "(() => { const input = document.getElementById('name'); const rect = input.getBoundingClientRect(); return { rect: [rect.x, rect.y, rect.width, rect.height], identity: [rect instanceof DOMRect, input.getClientRects().length === 1], scroll: [window.scrollX, window.scrollY, window.pageYOffset] }; })()",
        )
        .await
        .unwrap();
    assert_eq!(before_scroll["identity"], serde_json::json!([true, true]));
    assert!(before_scroll["rect"][1].as_i64().unwrap_or_default() > 0);
    assert_eq!(before_scroll["scroll"], serde_json::json!([0, 0, 0]));
    engine
        .action_async(NativeAction::Scroll {
            delta_x: 0,
            delta_y: 40,
        })
        .await
        .unwrap();
    let after_scroll = engine
        .evaluate_async(
            "[document.getElementById('name').getBoundingClientRect().y, window.scrollY]",
        )
        .await
        .unwrap();
    assert_eq!(
        after_scroll[0].as_i64(),
        Some(before_scroll["rect"][1].as_i64().unwrap() - 40)
    );
    assert_eq!(after_scroll[1], serde_json::json!(40));
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const input = document.getElementById('name'); const records = []; const observer = new MutationObserver(entries => records.push(entries.map(entry => [entry.type, entry.target.id, entry.oldValue]))); observer.observe(input, { attributes: true, attributeOldValue: true }); input.setAttribute('data-ready', 'yes'); globalThis.contentMutationRecords = records; return true; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.contentMutationRecords")
            .await
            .unwrap(),
        serde_json::json!([[["attributes", "name", serde_json::Value::Null]]])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_dynamic_inline_script_runs_once_after_late_attachment() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<html><body><p>Native content</p></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.dynamicLog = []; globalThis.dynamicScript = document.createElement('script'); dynamicScript.textContent = \"dynamicLog.push('ran'); document.body.setAttribute('data-dynamic', 'yes');\"; true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("[dynamicLog, document.body.getAttribute('data-dynamic')]")
            .await
            .unwrap(),
        serde_json::json!([[], serde_json::Value::Null])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "document.body.appendChild(dynamicScript); [dynamicLog, document.body.getAttribute('data-dynamic')]",
            )
            .await
            .unwrap(),
        serde_json::json!([["ran"], "yes"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "document.body.appendChild(dynamicScript); dynamicScript.textContent = \"dynamicLog.push('reran')\"; dynamicLog",
            )
            .await
            .unwrap(),
        serde_json::json!(["ran"])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_runs_nested_dynamic_external_scripts() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (content_type, body) = match path {
                "/page" => (
                    "text/html",
                    "<html><body><p>Native content</p></body></html>",
                ),
                "/parent.mjs" => (
                    "application/javascript",
                    "const child = document.createElement('script'); child.src = '/child.js'; document.body.appendChild(child);",
                ),
                "/child.js" => (
                    "application/javascript",
                    "document.body.setAttribute('data-nested-script', 'loaded');",
                ),
                other => panic!("unexpected request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.nestedScript = document.createElement('script'); nestedScript.type = 'module'; nestedScript.src = '/parent.mjs'; document.body.appendChild(nestedScript); document.body.getAttribute('data-nested-script')",
            )
            .await
            .unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(
        engine
            .evaluate_async("document.body.getAttribute('data-nested-script')")
            .await
            .unwrap(),
        serde_json::json!("loaded")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_scrolls_root_and_nested_overflow_containers() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/nested-scroll"));
        let body = "<html><head><style>body{margin:0px}</style></head><body><div id='scroller' style='width:40px;height:30px;overflow:auto'><div id='content' style='width:80px;height:90px;background-color:blue;position:relative'><div id='marker' style='position:absolute;left:30px;top:10px;width:10px;height:10px;background-color:red'></div></div></div><div id='tail' style='height:140px'></div></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 120,
                height: 100,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/nested-scroll")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); const events = []; scroller.addEventListener('scroll', event => events.push(['scroller', event.target === scroller, event.currentTarget === scroller, event.bubbles])); window.addEventListener('scroll', event => events.push(['window', event.target === window, event.currentTarget === window, event.bubbles])); document.addEventListener('scroll', event => events.push(['document', event.target === document, event.bubbles])); globalThis.scrollEvents = events; return true; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); return [scroller.clientWidth, scroller.clientHeight, scroller.scrollWidth, scroller.scrollHeight, scroller.scrollLeft, scroller.scrollTop, window.scrollY]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([40, 30, 80, 90, 0, 0, 0])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); scroller.scrollTo(20, 25); window.scrollTo(0, 40); return [scroller.scrollLeft, scroller.scrollTop, window.scrollY]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([20, 25, 40])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.scrollEvents")
            .await
            .unwrap(),
        serde_json::json!([
            ["scroller", true, true, false],
            ["window", true, true, false]
        ])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const content = document.getElementById('content').getBoundingClientRect(); const marker = document.getElementById('marker').getBoundingClientRect(); const scroller = document.getElementById('scroller'); return { content: [content.x, content.y], marker: [marker.x, marker.y], scroll: [scroller.scrollLeft, scroller.scrollTop, window.scrollY] }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "content": [-20, -65],
            "marker": [10, -55],
            "scroll": [20, 25, 40],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); scroller.scrollTop = 10; document.body.setAttribute('data-refresh', 'yes'); return true; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[document.getElementById('scroller').scrollTop, document.body.getAttribute('data-refresh')]",
            )
            .await
            .unwrap(),
        serde_json::json!([10, "yes"])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_http_window_proxy_close_removes_content_process_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = match path {
                "/close-parent" => {
                    "<title>HTTP close parent</title><script>globalThis.popup = window.open('/close-child', 'close-target');</script><p>parent</p>"
                }
                "/close-child" => "<title>HTTP close child</title><p>child</p>",
                other => panic!("unexpected window.close request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/close-parent")),
    )
    .await
    .unwrap();
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
    server.await.unwrap();
        });
    });
}
#[tokio::test]
async fn native_content_process_evaluates_persistent_script_realm() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "<html><head><title>Script</title></head><body><p id='copy'>Native <span id='old-copy'>script</span> page</p></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("6 * 7").await.unwrap(),
        serde_json::json!(42)
    );
    assert_eq!(
        engine.evaluate_async("document.title").await.unwrap(),
        serde_json::json!("Script")
    );
    assert_eq!(
        engine.evaluate_async("document.readyState").await.unwrap(),
        serde_json::json!("complete")
    );
    assert_eq!(
        engine.evaluate_async("location.origin").await.unwrap(),
        serde_json::json!(format!("http://{address}"))
    );
    assert_eq!(
        engine
            .evaluate_async("document.querySelector('p').textContent")
            .await
            .unwrap(),
        serde_json::json!("Native script page")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const paragraph = document.getElementById('copy'); const first = paragraph.firstChild; const old = document.getElementById('old-copy'); const last = paragraph.lastChild; return [first.nodeType, first.nodeValue, first.parentNode === paragraph, first.nextSibling === old, last.nodeType, last.nodeValue, paragraph.childNodes instanceof NodeList, paragraph.children instanceof HTMLCollection, paragraph.childNodes.length, paragraph.children.length, paragraph.children[0] === old]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([3, "Native ", true, true, 3, " page", true, true, 3, 1, true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const paragraph = document.querySelector('body > p#copy'); const old = document.querySelector('p#copy > span#old-copy'); const classes = paragraph.classList; classes.add('article', 'featured'); classes.toggle('selected', true); return [paragraph.matches('p#copy.article'), paragraph.closest('body') === document.body, paragraph.querySelector('span#old-copy') === old, paragraph.querySelectorAll('span, #old-copy').length, paragraph.getElementsByClassName('featured').length, classes.length, classes.contains('article'), classes.contains('selected'), paragraph.getAttribute('class')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, true, 1, 0, 3, true, true, "article featured selected"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const paragraph = document.getElementById('copy'); const style = paragraph.style; style.setProperty('color', 'red'); style.width = '120px'; style.setProperty('display', 'block', 'IMPORTANT'); const removed = style.removeProperty('color'); const dataset = paragraph.dataset; dataset.articleId = 42; dataset.enabled = true; const names = Object.keys(dataset).sort(); delete dataset.enabled; const detached = document.createElement('aside'); detached.dataset.kind = 'detached'; detached.style.display = 'block'; return [removed, style.width, style.getPropertyPriority('display'), style.item(0), style.length, style.cssText, dataset.articleId, dataset.enabled, names, paragraph.getAttribute('data-article-id'), paragraph.hasAttribute('data-enabled'), paragraph.style === style, detached.dataset.kind, detached.style.display]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "red",
            "120px",
            "important",
            "width",
            2,
            "width: 120px; display: block !important;",
            "42",
            null,
            ["articleId", "enabled"],
            "42",
            false,
            true,
            "detached",
            "block",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("(() => { const paragraph = document.getElementById('copy'); paragraph.textContent = 'Updated & literal'; return [paragraph.textContent, paragraph.innerText]; })()")
            .await
            .unwrap(),
        serde_json::json!(["Updated & literal", "Updated & literal"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[document.getElementById('copy').textContent, document.getElementById('old-copy')]"
            )
            .await
            .unwrap(),
        serde_json::json!(["Updated & literal", null])
    );
    assert_eq!(
        engine
            .evaluate_async("(() => { const paragraph = document.getElementById('copy'); paragraph.innerHTML = \"<strong id='new-copy'>Updated &amp; literal</strong>\"; return [paragraph.innerHTML, paragraph.textContent]; })()")
            .await
            .unwrap(),
        serde_json::json!(["<strong id=\"new-copy\">Updated &amp; literal</strong>", "Updated & literal"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const copy = document.getElementById('copy'); const first = copy.firstChild; return [copy.innerHTML, copy.textContent, document.getElementById('old-copy'), document.getElementById('new-copy').textContent, first.nodeType, first.nodeValue, first.parentNode === copy, copy.childNodes.length, copy.children.length]; })()"
            )
            .await
            .unwrap(),
        serde_json::json!(["<strong id=\"new-copy\">Updated &amp; literal</strong>", "Updated & literal", null, "Updated & literal", 1, null, true, 1, 1])
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('new-copy').remove(); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('new-copy')")
            .await
            .unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const host = document.createElement('article'); host.setAttribute('id', 'created-copy'); const child = document.createElement('b'); const text = document.createTextNode('created'); text.textContent = 'created'; child.appendChild(text); host.appendChild(child); document.body.appendChild(host); const children = host.children; const nodes = host.childNodes; const extra = document.createElement('i'); const extraText = document.createTextNode('!'); extra.appendChild(extraText); host.insertBefore(extra, child); const inserted = [children.length, nodes.length, host.firstChild === extra, host.lastChild === child, extra.firstChild === extraText, extraText.parentNode === extra, host.contains(extraText), document.contains(host)]; host.replaceChild(child, extra); return [host.parentElement === document.body, child.parentElement === host, text.parentNode === child, host.textContent, host.innerHTML, inserted, children.length, nodes.length, host.firstChild === child, host.lastChild === child]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, true, "created", "<b>created</b>", [2, 2, true, true, true, true, true, true], 1, 1, true, true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const host = document.getElementById('created-copy'); const first = host.firstChild; return [host.innerHTML, host.textContent, first && first.tagName, first && first.nodeType, first && first.parentNode === host, host.children.length, host.childNodes.length, host.firstElementChild === first, host.lastChild === first]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["<b>created</b>", "created", "B", 1, true, 1, 1, true, true])
    );
    assert_eq!(
        engine
            .evaluate_async("await Promise.resolve(42)")
            .await
            .unwrap(),
        serde_json::json!(42)
    );
    let pending_error = engine
        .evaluate_async("await new Promise(() => {})")
        .await
        .unwrap_err();
    assert!(
        pending_error
            .to_string()
            .contains("top-level await remained pending")
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.answer = (globalThis.answer || 0) + 1; answer")
            .await
            .unwrap(),
        serde_json::json!(1)
    );
    assert_eq!(
        engine.evaluate_async("answer").await.unwrap(),
        serde_json::json!(1)
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_enforces_inline_csp_for_initial_and_dynamic_content() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/csp"));
        let body = "<body><style nonce='style-ok'>#allowed { color: red; }</style><style>#blocked { color: blue; }</style><div id='allowed'>Allowed</div><div id='blocked'>Blocked</div><div id='attribute' style='color: green'>Attribute</div><script nonce='script-ok'>globalThis.cspEvents = ['allowed']; document.getElementsByTagName('script')[1].addEventListener('error', () => cspEvents.push('error'));</script><script>globalThis.cspEvents.push('blocked');</script></body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src 'none'; script-src 'nonce-script-ok'; style-src 'none'; style-src-elem 'nonce-style-ok'; style-src-attr 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/csp")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ events: globalThis.cspEvents, allowed: getComputedStyle(document.getElementById('allowed')).color, blocked: getComputedStyle(document.getElementById('blocked')).color, attribute: getComputedStyle(document.getElementById('attribute')).color })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["allowed", "error"],
            "allowed": "rgb(255, 0, 0)",
            "blocked": "rgb(0, 0, 0)",
            "attribute": "rgb(0, 0, 0)",
        })
    );
    engine
        .evaluate_async(
            "(() => { const script = document.createElement('script'); script.addEventListener('error', () => cspEvents.push('error')); script.textContent = \"globalThis.cspEvents.push('dynamic-blocked')\"; document.body.appendChild(script); const element = document.createElement('div'); element.id = 'dynamic-attribute'; element.setAttribute('style', 'color: purple'); document.body.appendChild(element); return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ events: globalThis.cspEvents, source: document.getElementById('dynamic-attribute').getAttribute('style'), color: getComputedStyle(document.getElementById('dynamic-attribute')).color })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["allowed", "error", "error"],
            "source": "color: purple",
            "color": "rgb(0, 0, 0)",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_applies_strict_dynamic_to_parser_and_dynamic_scripts() {
    let _guard = native_content_process_test_lock().lock().await;
    let script_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let script_address = script_listener.local_addr().unwrap();
    let script_server = tokio::spawn(async move {
        let (mut stream, _) = script_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/bootstrap.js"));
        let bootstrap = format!(
            "globalThis.strictDynamicEvents = ['bootstrap']; document.addEventListener('DOMContentLoaded', () => {{ const script = document.createElement('script'); script.src = 'http://{script_address}/dynamic.js'; document.getElementById('anchor').appendChild(script); }});"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bootstrap}",
            bootstrap.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = script_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/dynamic.js"));
        let body = "strictDynamicEvents.push('dynamic');";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        match tokio::time::timeout(Duration::from_millis(100), script_listener.accept()).await {
            Err(_) => {}
            Ok(Ok((_, peer))) => panic!("parser-blocked.js was unexpectedly requested by {peer}"),
            Ok(Err(error)) => {
                panic!("script listener failed while checking blocked parser script: {error}")
            }
        }
    });

    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let page_server = tokio::spawn(async move {
        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/strict-dynamic"));
        let body = format!(
            "<html><body><div id='anchor'></div><script src='http://{script_address}/bootstrap.js' nonce='boot-ok'></script><script src='http://{script_address}/parser-blocked.js'></script></body></html>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src 'none'; script-src 'strict-dynamic' 'nonce-boot-ok' https:\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{page_address}/strict-dynamic")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.strictDynamicEvents")
            .await
            .unwrap(),
        serde_json::json!(["bootstrap", "dynamic"])
    );
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    script_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_intersects_header_and_head_meta_csp() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/csp-meta"));
        let body = r#"<head><meta http-equiv="Content-Security-Policy" content="script-src 'nonce-meta-ok'"><meta http-equiv="Content-Security-Policy" content="style-src-elem 'nonce-style-ok'"><meta http-equiv="Content-Security-Policy-Report-Only" content="script-src 'none'"><style nonce="style-ok">#allowed { color: red; }</style><style>#blocked { color: blue; }</style><script nonce="meta-ok">globalThis.metaEvents = ['allowed'];</script><script>globalThis.metaEvents.push('blocked');</script></head><body><div id="allowed">Allowed</div><div id="blocked">Blocked</div></body>"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/csp-meta")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ events: globalThis.metaEvents, allowed: getComputedStyle(document.getElementById('allowed')).color, blocked: getComputedStyle(document.getElementById('blocked')).color })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["allowed"],
            "allowed": "rgb(255, 0, 0)",
            "blocked": "rgb(0, 0, 0)",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_appends_dynamically_inserted_head_meta_csp() {
    let _guard = native_content_process_test_lock().lock().await;
    let script_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let script_address = script_listener.local_addr().unwrap();
    let script_server = tokio::spawn(async move {
        match tokio::time::timeout(Duration::from_millis(250), script_listener.accept()).await {
            Err(_) => false,
            Ok(Ok((mut stream, _))) => {
                let request = read_http_request(&mut stream).await;
                assert_eq!(request.split_whitespace().nth(1), Some("/dynamic.js"));
                true
            }
            Ok(Err(error)) => panic!("dynamic script listener failed: {error}"),
        }
    });

    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let page_server = tokio::spawn(async move {
        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/dynamic-meta"));
        let body = format!(
            "<html><head><script>const meta = document.createElement('meta'); meta.setAttribute('http-equiv', 'Content-Security-Policy'); meta.setAttribute('content', \"script-src 'none'\"); document.head.appendChild(meta); meta.setAttribute('content', \"script-src 'unsafe-inline'\"); meta.remove(); const script = document.createElement('script'); script.src = 'http://{script_address}/dynamic.js'; document.head.appendChild(script); globalThis.metaAppendRan = true;</script></head><body></body></html>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{page_address}/dynamic-meta")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.metaAppendRan")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    assert!(!script_server.await.unwrap());
}
#[tokio::test]
async fn native_content_process_ignores_post_parse_csp_meta_content_edits() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/edited-meta"));
        let body = r#"<html><head><meta http-equiv="Content-Security-Policy" content="script-src 'unsafe-inline'"><script>const meta = document.querySelector('meta[http-equiv="Content-Security-Policy"]'); meta.setAttribute('content', "script-src 'none'"); const script = document.createElement('script'); script.textContent = 'globalThis.metaEditRan = true;'; document.head.appendChild(script);</script></head><body></body></html>"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/edited-meta")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ ran: globalThis.metaEditRan, content: document.querySelector('meta[http-equiv=\"Content-Security-Policy\"]').getAttribute('content') })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "ran": true,
            "content": "script-src 'none'",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_enforces_script_src_attr_for_initial_and_dynamic_handlers() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let allowed_source = "globalThis.attrEvents.push(this.id + ':' + event.type); return false;";
    let allowed_hash =
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(allowed_source.as_bytes()));
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/csp-attr"));
        let body = format!(
            "<body><button id='allowed' onclick=\"{allowed_source}\">Allowed</button><button id='blocked' onclick=\"globalThis.attrEvents.push('blocked')\">Blocked</button></body>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src 'none'; script-src 'nonce-only'; script-src-attr 'unsafe-hashes' 'sha256-{allowed_hash}'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/csp-attr")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.attrEvents = []; const allowed = document.getElementById('allowed'); allowed.addEventListener('click', event => attrEvents.push('default:' + event.defaultPrevented)); allowed.click(); document.getElementById('blocked').click(); attrEvents",
            )
            .await
            .unwrap(),
        serde_json::json!(["allowed:click", "default:true"])
    );
    engine
        .evaluate_async(&format!(
            "(() => {{ const button = document.createElement('button'); button.id = 'dynamic'; button.setAttribute('onclick', {allowed_source:?}); document.body.appendChild(button); return true; }})()"
        ))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('dynamic').click(); attrEvents")
            .await
            .unwrap(),
        serde_json::json!(["allowed:click", "default:true", "dynamic:click"])
    );
    assert_eq!(
        engine
        .evaluate_async(
            "(() => { const button = document.getElementById('dynamic'); button.setAttribute('onclick', \"globalThis.attrEvents.push('dynamic-blocked')\"); button.click(); button.removeAttribute('onclick'); button.click(); return attrEvents; })()",
        )
        .await
        .unwrap(),
        serde_json::json!(["allowed:click", "default:true", "dynamic:click"])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_scheduled_callback_errors_do_not_abort_sibling_work() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = r#"
            <script>
                globalThis.eventErrors = [];
                globalThis.handlerErrors = [];
                globalThis.callbackErrors = {};
                globalThis.timerSurvived = false;
                window.addEventListener('error', event => eventErrors.push([
                    event.message,
                    event.error instanceof Error,
                    event.target === window,
                    event.error === callbackErrors[event.message],
                ]));
                window.onerror = (message, filename, line, column, error) => handlerErrors.push([
                    message,
                    error instanceof Error,
                    error === callbackErrors[message],
                ]);
                setTimeout(() => { const error = new Error('content timer callback boom'); callbackErrors[error.message] = error; throw error; }, 0);
                setTimeout(() => { globalThis.timerSurvived = true; }, 0);
                requestIdleCallback(() => { const error = new Error('content idle callback boom'); callbackErrors[error.message] = error; throw error; });
                requestAnimationFrame(() => { const error = new Error('content frame callback boom'); callbackErrors[error.message] = error; throw error; });
            </script>
            <p>Content callbacks remain isolated</p>
        "#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        engine
            .evaluate_async("({ eventErrors: eventErrors.slice().sort((left, right) => left[0].localeCompare(right[0])), handlerErrors: handlerErrors.slice().sort((left, right) => left[0].localeCompare(right[0])), timerSurvived })")
            .await
            .unwrap(),
        serde_json::json!({
            "eventErrors": [
                ["content frame callback boom", true, true, true],
                ["content idle callback boom", true, true, true],
                ["content timer callback boom", true, true, true],
            ],
            "handlerErrors": [
                ["content frame callback boom", true, true],
                ["content idle callback boom", true, true],
                ["content timer callback boom", true, true],
            ],
            "timerSurvived": true,
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_interval_reschedules_until_cleared() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<script>globalThis.intervalCount = 0; globalThis.intervalId = setInterval(() => { globalThis.intervalCount += 1; }, 40);</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let initial_count = engine
        .evaluate_async("globalThis.intervalCount")
        .await
        .unwrap()
        .as_u64()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let first_count = engine
        .evaluate_async("globalThis.intervalCount")
        .await
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(first_count > initial_count);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let second_count = engine
        .evaluate_async("globalThis.intervalCount")
        .await
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(second_count > first_count);
    engine
        .evaluate_async("clearInterval(globalThis.intervalId)")
        .await
        .unwrap();
    let cleared_count = engine
        .evaluate_async("globalThis.intervalCount")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(
        engine
            .evaluate_async("globalThis.intervalCount")
            .await
            .unwrap(),
        cleared_count
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_honors_bounded_async_and_defer_script_order() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/page",
            "/blocking.js",
            "/defer.js",
            "/async.js",
            "/blocking-two.js",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = match expected_path {
                "/page" => (
                    "text/html",
                    "<script src='/blocking.js'></script><script defer src='/defer.js'></script><script async src='/async.js'></script><script src='/blocking-two.js'></script>",
                ),
                "/blocking.js" => (
                    "application/javascript",
                    "globalThis.parserOrder = 'blocking-1';",
                ),
                "/defer.js" => (
                    "application/javascript",
                    "globalThis.parserOrder += '-defer';",
                ),
                "/async.js" => (
                    "application/javascript",
                    "globalThis.parserOrder += '-async';",
                ),
                _ => (
                    "application/javascript",
                    "globalThis.parserOrder += '-blocking-2';",
                ),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.parserOrder")
            .await
            .unwrap(),
        serde_json::json!("blocking-1-async-blocking-2-defer")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_keeps_fragment_module_identities_and_response_bases() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let expected_paths = [
            "/page",
            "/entry.js",
            "/modules/entry-real.js",
            "/modules/dep.js",
            "/modules/dep.js",
        ];
        let mut requests = Vec::new();
        for expected_path in expected_paths {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("HTTP request includes a path")
                .to_owned();
            assert_eq!(path, expected_path);
            let response = match path.as_str() {
                "/entry.js" => {
                    "HTTP/1.1 302 Found\r\nLocation: /modules/entry-real.js\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()
                }
                "/page" => {
                    let body = "<script>globalThis.fragmentModuleErrors = []; addEventListener('error', event => fragmentModuleErrors.push(String(event.message)), true);</script><script type='module' src='/entry.js#root'></script>";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/modules/entry-real.js" => {
                    let body = "import './dep.js#first'; import './dep.js#second'; globalThis.fragmentModuleReady = true;";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/modules/dep.js" => {
                    let body = "globalThis.fragmentModuleExecutions = (globalThis.fragmentModuleExecutions || 0) + 1; export const execution = globalThis.fragmentModuleExecutions;";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                other => panic!("unexpected fragment-module request: {other}"),
            };
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push(path);
        }
        requests
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let result = engine
        .evaluate_async(
            "[globalThis.fragmentModuleExecutions, globalThis.fragmentModuleReady, globalThis.fragmentModuleErrors]",
        )
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        result,
        serde_json::json!([2, true, []]),
        "request URLs with distinct fragments must instantiate separately, while imports resolve from the redirected response URL"
    );
    assert_eq!(requests.len(), 5);
}
#[tokio::test]
async fn native_content_process_resolves_module_graphs_through_inline_import_maps() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        while requests.len() < 12 {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let accepted = tokio::time::timeout(remaining, listener.accept()).await;
            let (mut stream, _) = match accepted {
                Ok(Ok(accepted)) => accepted,
                Ok(Err(error)) => panic!("module-request listener failed: {error}"),
                Err(_) => return (requests, true),
            };
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("HTTP request includes a path")
                .to_owned();
            requests.push(path.clone());
            let (content_type, body) = match path.as_str() {
                "/page" => (
                    "text/html",
                    "<script>globalThis.moduleErrors = []; globalThis.addEventListener('error', event => globalThis.moduleErrors.push([String(event.message), String(event.error), event.error ? String(event.error.message) : 'no-error']), true);</script><script type='IMPORTMAP'>{\"imports\":{\"pkg\":\"/vendor/pkg.js\",\"@lib/\":\"/modules/\"},\"scopes\":{\"/vendor/\":{\"pkg\":\"/vendor/scoped-parent.js\"},\"/vendor/nested/\":{\"pkg\":\"/vendor/scoped-nested.js\"}}}</script><script src='/vendor/classic.js'></script><script type='module' src='/app.js'></script>",
                ),
                "/vendor/classic.js" => (
                    "application/javascript",
                    "globalThis.dynamicClassicError = null; const classicSpecifier = 'pkg'; import(classicSpecifier).then(({ value }) => { globalThis.dynamicClassic = value; }, error => { globalThis.dynamicClassicError = String(error); });",
                ),
                "/app.js" => (
                    "application/javascript",
                    "import { exact } from 'pkg'; import { prefix } from '@lib/suffix.js'; import { scoped as parent } from '/vendor/referrer.js'; import { scoped as nested } from '/vendor/nested/referrer.js'; globalThis.mappedModules = [exact, prefix, parent, nested].join('-'); import('@lib/dynamic.js').then(({ value }) => { globalThis.dynamicMapped = value; }); globalThis.dynamicRuntimeError = null; const dynamicSpecifier = '@lib/' + 'dynamic-runtime.js'; import(dynamicSpecifier).then(({ value }) => { globalThis.dynamicRuntimeMapped = value; }, error => { globalThis.dynamicRuntimeError = String(error); });",
                ),
                "/vendor/pkg.js" => ("application/javascript", "export const exact = 'exact';"),
                "/modules/suffix.js" => {
                    ("application/javascript", "export const prefix = 'prefix';")
                }
                "/vendor/referrer.js" => (
                    "application/javascript",
                    "import { value } from 'pkg'; export const scoped = value;",
                ),
                "/vendor/nested/referrer.js" => (
                    "application/javascript",
                    "import { value } from 'pkg'; export const scoped = value; import('pkg').then(({ value }) => { globalThis.dynamicScoped = value; });",
                ),
                "/vendor/scoped-parent.js" => {
                    ("application/javascript", "export const value = 'parent';")
                }
                "/vendor/scoped-nested.js" => {
                    ("application/javascript", "export const value = 'nested';")
                }
                "/modules/dynamic.js" => {
                    ("application/javascript", "export const value = 'dynamic';")
                }
                "/modules/dynamic-runtime.js" => (
                    "application/javascript",
                    "export const value = 'runtime-computed'; globalThis.dynamicRuntimeNestedError = null; const childSpecifier = './runtime-child.js'; import(childSpecifier).then(({ value }) => { globalThis.dynamicRuntimeNested = value; }, error => { globalThis.dynamicRuntimeNestedError = String(error); });",
                ),
                "/modules/runtime-child.js" => (
                    "application/javascript",
                    "export const value = 'runtime-nested';",
                ),
                _ => ("application/javascript", "export const value = 'dynamic';"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        (requests, false)
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mapped_modules = engine
        .evaluate_async("globalThis.mappedModules")
        .await
        .unwrap();
    let dynamic_mapped = engine
        .evaluate_async("globalThis.dynamicMapped")
        .await
        .unwrap();
    let dynamic_classic = engine
        .evaluate_async("globalThis.dynamicClassic")
        .await
        .unwrap();
    let dynamic_classic_error = engine
        .evaluate_async("globalThis.dynamicClassicError")
        .await
        .unwrap();
    let dynamic_scoped = engine
        .evaluate_async("globalThis.dynamicScoped")
        .await
        .unwrap();
    let dynamic_runtime = engine
        .evaluate_async("globalThis.dynamicRuntimeMapped")
        .await
        .unwrap();
    let dynamic_runtime_nested = engine
        .evaluate_async("globalThis.dynamicRuntimeNested")
        .await
        .unwrap();
    let dynamic_runtime_error = engine
        .evaluate_async("globalThis.dynamicRuntimeError")
        .await
        .unwrap();
    let dynamic_runtime_nested_error = engine
        .evaluate_async("globalThis.dynamicRuntimeNestedError")
        .await
        .unwrap();
    let module_errors = engine
        .evaluate_async("globalThis.moduleErrors")
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    let (mut requests, request_timeout) = server.await.unwrap();
    requests.sort();
    let mut expected_requests = vec![
        "/page",
        "/app.js",
        "/vendor/classic.js",
        "/vendor/pkg.js",
        "/modules/suffix.js",
        "/vendor/referrer.js",
        "/vendor/nested/referrer.js",
        "/vendor/scoped-parent.js",
        "/vendor/scoped-nested.js",
        "/modules/dynamic.js",
        "/modules/dynamic-runtime.js",
        "/modules/runtime-child.js",
    ];
    expected_requests.sort();
    assert_eq!(
        module_errors,
        serde_json::json!([]),
        "requests: {requests:?}"
    );
    assert_eq!(
        mapped_modules,
        serde_json::json!("exact-prefix-parent-nested"),
        "script errors: {module_errors:?}; requests: {requests:?}"
    );
    assert_eq!(dynamic_mapped, serde_json::json!("dynamic"));
    assert_eq!(dynamic_classic, serde_json::json!("parent"));
    assert_eq!(dynamic_classic_error, serde_json::Value::Null);
    assert_eq!(dynamic_scoped, serde_json::json!("nested"));
    assert_eq!(
        dynamic_runtime,
        serde_json::json!("runtime-computed"),
        "dynamic import rejection: {dynamic_runtime_error:?}; requests: {requests:?}"
    );
    assert_eq!(
        dynamic_runtime_nested,
        serde_json::json!("runtime-nested"),
        "nested import rejection: {dynamic_runtime_nested_error:?}; requests: {requests:?}"
    );
    assert_eq!(dynamic_runtime_error, serde_json::Value::Null);
    assert_eq!(dynamic_runtime_nested_error, serde_json::Value::Null);
    assert!(
        !request_timeout,
        "module request server timed out after receiving: {requests:?}"
    );
    assert_eq!(requests, expected_requests);
}
#[tokio::test]
async fn native_content_process_uses_dynamic_import_map_for_a_later_module_root() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut requests = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener
                    .accept()
                    .await
                    .expect("dynamic import-map listener remains available");
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("HTTP request includes a path")
                    .to_owned();
                let (content_type, body) = match path.as_str() {
                    "/page" => (
                        "text/html",
                        "<html><head></head><body><p>Dynamic map</p></body></html>",
                    ),
                    "/mapped.js" => (
                        "application/javascript",
                        "export const value = 'dynamic-map';",
                    ),
                    other => panic!("unexpected dynamic import-map request: {other}"),
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push(path);
            }
            requests
        })
        .await
        .expect("dynamic import-map fixture stays within its time bound")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const map = document.createElement('script');
                    map.type = 'importmap';
                    map.textContent = JSON.stringify({ imports: { 'dynamic-package': '/mapped.js' } });
                    document.head.appendChild(map);
                    return true;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const module = document.createElement('script');
                    module.type = 'module';
                    module.textContent = "import { value } from 'dynamic-package'; globalThis.dynamicMapValue = value;";
                    document.head.appendChild(module);
                    return true;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.dynamicMapValue")
            .await
            .unwrap(),
        serde_json::json!("dynamic-map")
    );
    engine.close_async().await.unwrap();

    let requests = server.await.unwrap();
    assert_eq!(requests, ["/page", "/mapped.js"]);
}
#[tokio::test]
async fn native_content_process_locks_module_resolutions_between_parser_import_maps() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut requests = Vec::new();
            for _ in 0..6 {
                let (mut stream, _) = listener
                    .accept()
                    .await
                    .expect("ordered import-map fixture listener remains available");
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("HTTP request includes a path")
                    .to_owned();
                let (content_type, body) = match path.as_str() {
                    "/page" => (
                        "text/html",
                        "<script>globalThis.orderedValues = []; globalThis.addEventListener('error', event => globalThis.orderedValues.push(String(event.message)), true);</script><script type='importmap'>{\"imports\":{\"pkg/\":\"/v1/\"}}</script><script type='module' src='/early.js'></script><script type='importmap'>{\"imports\":{\"pkg/item.js\":\"/v2/item.js\",\"later\":\"/later.js\"}}</script><script type='module' src='/late.js'></script>",
                    ),
                    "/early.js" => (
                        "application/javascript",
                        "import { value } from 'pkg/item.js'; globalThis.earlyValue = value;",
                    ),
                    "/late.js" => (
                        "application/javascript",
                        "import { value } from 'pkg/item.js'; import { later } from 'later'; globalThis.lateValues = value + '-' + later;",
                    ),
                    "/v1/item.js" => (
                        "application/javascript",
                        "export const value = 'v1';",
                    ),
                    "/later.js" => (
                        "application/javascript",
                        "export const later = 'unrelated';",
                    ),
                    _ => panic!("unexpected ordered import-map request {path}"),
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push(path);
            }
            requests
        })
        .await
        .expect("the ordered import-map fixture stays within its time bound")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let early_value = engine
        .evaluate_async("globalThis.earlyValue")
        .await
        .unwrap();
    let late_values = engine
        .evaluate_async("globalThis.lateValues")
        .await
        .unwrap();
    let module_errors = engine
        .evaluate_async("globalThis.orderedValues")
        .await
        .unwrap();
    engine.close_async().await.unwrap();

    let requests = server.await.unwrap();
    assert_eq!(
        requests.iter().map(String::as_str).collect::<Vec<_>>(),
        [
            "/page",
            "/early.js",
            "/v1/item.js",
            "/late.js",
            "/v1/item.js",
            "/later.js",
        ]
    );
    assert_eq!(early_value, serde_json::json!("v1"));
    assert_eq!(late_values, serde_json::json!("v1-unrelated"));
    assert_eq!(module_errors, serde_json::json!([]));
}
#[tokio::test]
async fn native_content_process_enforces_import_map_integrity_before_module_execution() {
    let _guard = native_content_process_test_lock().lock().await;
    let document_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let document_address = document_listener.local_addr().unwrap();
    let document_origin = format!("http://{document_address}");
    let module_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let module_address = module_listener.local_addr().unwrap();

    let good_source = "export const value = 'verified';";
    let good_digest =
        base64::engine::general_purpose::STANDARD.encode(Sha384::digest(good_source.as_bytes()));
    let bad_digest = base64::engine::general_purpose::STANDARD.encode([0_u8; 32]);
    let module_document_origin = document_origin.clone();
    let module_server = tokio::spawn(async move {
        let mut paths = Vec::new();
        for _ in 0..4 {
            let (mut stream, _) = module_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("module request includes a path")
                .to_owned();
            assert!(request.lines().any(|line| {
                line.eq_ignore_ascii_case(&format!("origin: {module_document_origin}"))
            }));
            assert!(!request.lines().any(|line| {
                line.split_once(':')
                    .is_some_and(|(name, _)| name.eq_ignore_ascii_case("cookie"))
            }));
            let body = match path.as_str() {
                "/good-entry.mjs" => {
                    "import { value } from './good.js'; globalThis.integrityGood = value;"
                }
                "/bad-entry.mjs" => {
                    "import { value } from './bad.js'; globalThis.integrityBad = value;"
                }
                "/good.js" => good_source,
                "/bad.js" => "export const value = 'tampered';",
                _ => panic!("unexpected module request {path}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nAccess-Control-Allow-Origin: {module_document_origin}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            paths.push(path);
        }
        paths
    });

    let import_map = format!(
        r#"{{"integrity":{{"http://{module_address}/good.js":"sha384-{good_digest}","http://{module_address}/bad.js":"sha256-{bad_digest}"}}}}"#
    );
    let html = format!(
        "<script type='importmap'>{import_map}</script><script type='module' src='http://{module_address}/good-entry.mjs'></script><script type='module' src='http://{module_address}/bad-entry.mjs'></script>"
    );
    let document_server = tokio::spawn(async move {
        let (mut stream, _) = document_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: session=private; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
            html.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{document_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.integrityGood")
            .await
            .unwrap(),
        serde_json::json!("verified")
    );
    assert_eq!(
        engine
            .evaluate_async("Boolean(globalThis.integrityBad)")
            .await
            .unwrap(),
        serde_json::json!(false)
    );
    engine.close_async().await.unwrap();

    let mut paths = module_server.await.unwrap();
    paths.sort();
    assert_eq!(
        paths,
        vec!["/bad-entry.mjs", "/bad.js", "/good-entry.mjs", "/good.js",]
    );
    document_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_isolates_static_module_dependency_failure() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/app.js", "/missing.js"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/missing.js" {
                stream
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            } else {
                let body = if expected_path == "/page" {
                    "<script id='module' type='module' src='/app.js'></script><title>Module failure is isolated</title><p>Document committed</p>"
                } else {
                    "import { value } from './missing.js'; globalThis.moduleRan = value;"
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    if expected_path == "/page" {
                        "text/html"
                    } else {
                        "application/javascript"
                    },
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.url, format!("http://{address}/page"));
    assert_eq!(snapshot.title, "Module failure is isolated");
    assert_eq!(snapshot.visible_text, "Document committed");
    assert_eq!(
        engine
            .evaluate_async("Boolean(globalThis.moduleRan)")
            .await
            .unwrap(),
        serde_json::json!(false)
    );
    assert_eq!(
        engine
            .effects_since(0)
            .unwrap()
            .effects
            .iter()
            .filter(|effect| effect.kind == NativeEventKind::Error)
            .count(),
        1
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_resolves_static_expression_dynamic_imports() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/app.js", "/dep.js", "/other.js"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = match expected_path {
                "/page" => ("text/html", "<script type='module' src='/app.js'></script>"),
                "/app.js" => (
                    "application/javascript",
                    "globalThis.dynamicValue = 'pending'; if (false) import('./never-requested.js'); Promise.all([import('./' + /* static expression */ 'dep.js'), import(( './' + 'other.js' ))]).then(([first, second]) => { globalThis.dynamicValue = [first.value, second.value]; });",
                ),
                _ => (
                    "application/javascript",
                    if expected_path == "/dep.js" {
                        "export const value = 'dynamic-dep';"
                    } else {
                        "export const value = 'other-dynamic-dep';"
                    },
                ),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.dynamicValue")
            .await
            .unwrap(),
        serde_json::json!(["dynamic-dep", "other-dynamic-dep"])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_import_scripts_dynamic_imports_use_final_script_urls() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(25), async {
            let mut requests = Vec::new();
            for _ in 0..10 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("classic Worker request includes a URL")
                    .to_owned();
                let (status, extra_headers, content_type, body) = match path.as_str() {
                    "/page" => (
                        "200 OK",
                        "",
                        "text/html",
                        "<script>globalThis.workerMessages = []; globalThis.workerErrors = []; const watch = worker => { worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(String(event.message)); }; globalThis.dedicated = new Worker('/workers/dedicated.js'); watch(dedicated); globalThis.shared = new SharedWorker('/workers/shared.js'); shared.port.onmessage = event => workerMessages.push(event.data); shared.port.start();</script>",
                    ),
                    "/workers/dedicated.js" => (
                        "200 OK",
                        "",
                        "application/javascript",
                        "importScripts('/scripts/dedicated-bridge.js?redirect=1');",
                    ),
                    "/scripts/dedicated-bridge.js?redirect=1" => (
                        "302 Found",
                        "Location: /assets/dedicated/bridge.js?final=1\r\n",
                        "application/javascript",
                        "",
                    ),
                    "/assets/dedicated/bridge.js?final=1" => (
                        "200 OK",
                        "",
                        "application/javascript",
                        "const target = './computed.js?source=bridge'; import(target).then(async module => postMessage({ kind: 'dedicated', value: module.value, nested: await module.loadNested() }), error => postMessage({ kind: 'dedicated-error', message: String(error) }));",
                    ),
                    "/assets/dedicated/computed.js?source=bridge" => (
                        "200 OK",
                        "",
                        "application/javascript",
                        "export const value = 'dedicated-computed'; export async function loadNested() { const target = './nested.js?source=computed'; return (await import(target)).value; }",
                    ),
                    "/assets/dedicated/nested.js?source=computed" => (
                        "200 OK",
                        "",
                        "application/javascript",
                        "export const value = 'dedicated-nested';",
                    ),
                    "/workers/shared.js" => (
                        "200 OK",
                        "",
                        "application/javascript",
                        "importScripts('/assets/shared/bridge.js');",
                    ),
                    "/assets/shared/bridge.js" => (
                        "200 OK",
                        "",
                        "application/javascript",
                        "globalThis.onconnect = event => { const port = event.ports[0]; const target = './computed.js?source=bridge'; import(target).then(async module => port.postMessage({ kind: 'shared', value: module.value, nested: await module.loadNested() }), error => port.postMessage({ kind: 'shared-error', message: String(error) })); };",
                    ),
                    "/assets/shared/computed.js?source=bridge" => (
                        "200 OK",
                        "",
                        "application/javascript",
                        "export const value = 'shared-computed'; export async function loadNested() { const target = './nested.js?source=computed'; return (await import(target)).value; }",
                    ),
                    "/assets/shared/nested.js?source=computed" => (
                        "200 OK",
                        "",
                        "application/javascript",
                        "export const value = 'shared-nested';",
                    ),
                    other => panic!("unexpected importScripts dynamic-module request: {other}"),
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push(path);
            }
            requests.sort();
            requests
        })
        .await
        .expect("classic Worker importScripts dynamic imports stay within their time bound")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut result = serde_json::Value::Null;
    for _ in 0..8 {
        result = engine
            .evaluate_async(
                "({ messages: workerMessages.slice().sort((a, b) => String(a.kind).localeCompare(String(b.kind))), errors: workerErrors })",
            )
            .await
            .unwrap();
        if result["messages"]
            .as_array()
            .is_some_and(|messages| messages.len() == 2)
        {
            break;
        }
    }
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        result,
        serde_json::json!({
            "messages": [
                {"kind": "dedicated", "value": "dedicated-computed", "nested": "dedicated-nested"},
                {"kind": "shared", "value": "shared-computed", "nested": "shared-nested"},
            ],
            "errors": [],
        }),
        "computed imports inside preloaded classic Worker scripts must use each final script response URL and settle through their owning Worker"
    );
    assert_eq!(
        requests,
        vec![
            "/assets/dedicated/bridge.js?final=1",
            "/assets/dedicated/computed.js?source=bridge",
            "/assets/dedicated/nested.js?source=computed",
            "/assets/shared/bridge.js",
            "/assets/shared/computed.js?source=bridge",
            "/assets/shared/nested.js?source=computed",
            "/page",
            "/scripts/dedicated-bridge.js?redirect=1",
            "/workers/dedicated.js",
            "/workers/shared.js",
        ]
    );
}
#[tokio::test]
async fn native_content_process_applies_same_origin_referrer_policy() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/source", "/same"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let referrer = request.lines().find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("referer")
                        .then_some(value.trim().to_owned())
                })
            });
            if expected_path == "/source" {
                assert_eq!(referrer, None);
            } else {
                assert_eq!(referrer, Some(format!("http://{address}/source")));
            }
            let body = if expected_path == "/source" {
                "<title>Source</title><p>Source page</p>"
            } else {
                "<title>Same origin</title><p>Same-origin destination</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/source#fragment")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .navigate_async(format!("http://{address}/same"))
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().title, "Same origin");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_reduces_cross_origin_referrer_to_origin() {
    let _guard = native_content_process_test_lock().lock().await;
    let source_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source_address = source_listener.local_addr().unwrap();
    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    let source_server = tokio::spawn(async move {
        let (mut stream, _) = source_listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "<title>Source</title><p>Source page</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let target_server = tokio::spawn(async move {
        let (mut stream, _) = target_listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        let path = request
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_owned();
        let referrer = request.lines().find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("referer")
                    .then_some(value.trim().to_owned())
            })
        });
        let body = "<title>Cross origin</title><p>Target page</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        (path, referrer)
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{source_address}/source")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .navigate_async(format!("http://{target_address}/target"))
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().title, "Cross origin");
    engine.close_async().await.unwrap();
    source_server.await.unwrap();
    let (path, referrer) = target_server.await.unwrap();
    assert_eq!(path, "/target");
    assert_eq!(referrer, Some(format!("http://{source_address}/")));
}
#[tokio::test]
async fn native_content_process_decodes_declared_external_html_charset() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = b"<title>caf\xe9</title><p>Latin page</p>";
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=windows-1252\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/latin")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.title, "café");
    assert_eq!(snapshot.visible_text, "Latin page");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_revalidates_stale_http_cache_with_etag() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for request_index in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some("/revalidate"));
            if request_index == 0 {
                assert!(
                    !request
                        .lines()
                        .any(|line| line.eq_ignore_ascii_case("if-none-match: \"v1\""))
                );
                let body = "<title>Revalidated</title><p>Cached representation</p>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nCache-Control: no-cache\r\nETag: \"v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            } else {
                assert!(
                    request
                        .lines()
                        .any(|line| line.eq_ignore_ascii_case("if-none-match: \"v1\""))
                );
                stream
                    .write_all(
                        b"HTTP/1.1 304 Not Modified\r\nCache-Control: no-cache\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            }
        }
    });

    let base_url = format!("http://{address}");
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{base_url}/revalidate")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(engine.snapshot().unwrap().title, "Revalidated");
    engine
        .navigate_async(format!("{base_url}/revalidate"))
        .await
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.title, "Revalidated");
    assert_eq!(snapshot.visible_text, "Cached representation");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_jpeg_picture_source() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let jpeg = native_test_jpeg_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/photo.jpg"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/photo.jpg" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    jpeg.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&jpeg).await.unwrap();
            } else {
                let body = "<picture><source type='image/jpeg' srcset='/photo.jpg 1x'><img id='image' src='/fallback.png' alt='photo'></picture>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 240, 426, format!("http://{address}/photo.jpg")])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_webp_picture_source() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let webp = native_test_webp_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/photo.webp"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/photo.webp" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/webp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    webp.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&webp).await.unwrap();
            } else {
                let body = "<picture><source type='image/webp' srcset='/photo.webp 1x'><img id='image' src='/fallback.png' alt='photo'></picture>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, 2, format!("http://{address}/photo.webp")])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_gif_picture_source() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let gif = native_test_gif_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/photo.gif"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/photo.gif" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/gif\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    gif.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&gif).await.unwrap();
            } else {
                let body = "<picture><source type='image/gif' srcset='/photo.gif 1x'><img id='image' src='/fallback.png' alt='photo'></picture>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 1, 1, format!("http://{address}/photo.gif")])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_svg_picture_source() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let svg = native_test_svg_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/photo.svg"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/photo.svg" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    svg.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&svg).await.unwrap();
            } else {
                let body = "<picture><source type='image/svg+xml' srcset='/photo.svg 1x'><img id='image' src='/fallback.png' alt='photo'></picture>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 4, 3, format!("http://{address}/photo.svg")])
    );
    assert!(
        engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .any(|command| {
                matches!(
                    command,
                    NativeDisplayCommand::Image {
                        source_width: 4,
                        source_height: 3,
                        pixels,
                        ..
                    } if pixels.len() == 4 * 3 * 4
                )
            })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_applies_img_referrer_policy() {
    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let image_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let image_address = image_listener.local_addr().unwrap();
    let document_url = format!("http://{page_address}/page?source=policy#fragment");
    let document_referer = document_url.split('#').next().unwrap().to_owned();
    let same_origin_referer = document_referer.clone();
    let document_origin = format!("http://{page_address}/");
    let page_png = native_test_png_bytes();
    let image_png = page_png.clone();
    let page_server = tokio::spawn(async move {
        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/page?source=policy")
        );
        let body = format!(
            "<meta name='referrer' content='strict-origin-when-cross-origin'>\
             <img id='same-origin' src='/same.png'>\
             <img id='default-cross-origin' src='http://{image_address}/default.png'>\
             <img id='explicit-origin' src='http://{image_address}/origin.png' referrerpolicy='ORIGIN'>\
             <img id='no-referrer' src='http://{image_address}/no-referrer.png' referrerpolicy='no-referrer'>\
             <img id='invalid' src='http://{image_address}/invalid.png' referrerpolicy='not-a-policy'>\
             <img id='redirect' src='http://{image_address}/redirect.png' referrerpolicy='unsafe-url'>\
             <div id='dynamic'></div>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/same.png"));
        let actual_referer = request.lines().find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("referer")
                    .then(|| value.trim().to_owned())
            })
        });
        assert_eq!(
            actual_referer.as_deref(),
            Some(same_origin_referer.as_str())
        );
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            page_png.len()
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        stream.write_all(&page_png).await.unwrap();
    });

    let cross_origin_referer = document_origin.clone();
    let redirect_initial_referer = document_referer.clone();
    let redirect_target_referer = document_origin.clone();
    let dynamic_referer = document_referer.clone();
    let image_server = tokio::spawn(async move {
        let cases = [
            ("/default.png", Some(cross_origin_referer)),
            ("/origin.png", Some(document_origin.clone())),
            ("/no-referrer.png", None),
            ("/invalid.png", Some(document_origin)),
            ("/redirect.png", Some(redirect_initial_referer)),
            ("/redirect-target.png", Some(redirect_target_referer)),
            ("/dynamic.png", Some(dynamic_referer)),
        ];
        for (expected_path, expected_referer) in cases {
            let (mut stream, _) = image_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let actual_referer = request.lines().find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("referer")
                        .then(|| value.trim().to_owned())
                })
            });
            assert_eq!(actual_referer, expected_referer, "{expected_path}");
            if expected_path == "/redirect.png" {
                stream
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: /redirect-target.png\r\nReferrer-Policy: origin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            } else {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    image_png.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&image_png).await.unwrap();
            }
        }
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(document_url)).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(&format!(
                "(() => {{ const probe = document.createElement('img'); probe.setAttribute('referrerpolicy', 'not-a-policy'); const reflectedInvalid = probe.referrerPolicy; const image = document.createElement('img'); image.referrerPolicy = 'unsafe-url'; const reflected = [image.referrerPolicy, image.getAttribute('referrerpolicy')]; image.src = 'http://{image_address}/dynamic.png'; document.body.appendChild(image); return [reflectedInvalid, probe.getAttribute('referrerpolicy'), ...reflected]; }})()"
            ))
            .await
            .unwrap(),
        serde_json::json!(["", "not-a-policy", "unsafe-url", "unsafe-url"])
    );

    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    image_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_applies_classic_script_referrer_policy() {
    fn request_header(request: &str, name: &str) -> Option<String> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_owned())
            })
        })
    }

    fn script_response(body: &str, cache_control: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nCache-Control: {cache_control}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let script_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let script_address = script_listener.local_addr().unwrap();
    let document_url = format!("http://{page_address}/page?secret=token#fragment");
    let document_referer = format!("http://{page_address}/page?secret=token");
    let document_origin = format!("http://{page_address}/");
    let script_body = "globalThis.scriptReferrerLoads = (globalThis.scriptReferrerLoads || 0) + 1;";
    let page_document_referer = document_referer.clone();
    let page_script_body = script_body.to_owned();
    let page_server = tokio::spawn(async move {
        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/page?secret=token")
        );
        let body = format!(
            "<script id='same' src='/same.js'></script>\
             <script id='default' src='http://{script_address}/default.js'></script>\
             <script id='empty' src='http://{script_address}/empty.js' referrerpolicy=''></script>\
             <script id='origin' src='http://{script_address}/origin.js' referrerpolicy='ORIGIN'></script>\
             <script id='invalid' src='http://{script_address}/invalid.js' referrerpolicy='not-a-policy'></script>\
             <script id='none' src='http://{script_address}/none.js' referrerpolicy='no-referrer'></script>\
             <script id='redirect' src='http://{script_address}/redirect.js' referrerpolicy='no-referrer'></script>\
             <script id='cache' src='/cache.js'></script>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nReferrer-Policy: unsafe-url\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        for expected_path in ["/same.js", "/cache.js"] {
            let (mut stream, _) = page_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            assert_eq!(
                request_header(&request, "referer").as_deref(),
                Some(page_document_referer.as_str()),
                "{expected_path} initial fetch"
            );
            if expected_path == "/cache.js" {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nCache-Control: no-cache\r\nETag: \"script-v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    page_script_body.len(),
                    page_script_body
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            } else {
                stream
                    .write_all(script_response(&page_script_body, "no-store").as_bytes())
                    .await
                    .unwrap();
            }
        }

        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/cache.js"));
        assert_eq!(request_header(&request, "referer"), None);
        assert_eq!(
            request_header(&request, "if-none-match").as_deref(),
            Some("\"script-v1\"")
        );
        stream
            .write_all(
                b"HTTP/1.1 304 Not Modified\r\nCache-Control: no-cache\r\nETag: \"script-v1\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();

        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/dynamic-default.js")
        );
        assert_eq!(request_header(&request, "referer"), None);
        stream
            .write_all(script_response(&page_script_body, "no-store").as_bytes())
            .await
            .unwrap();
    });

    let cross_origin_referer = document_origin.clone();
    let full_referer = document_referer.clone();
    let script_body = script_body.to_owned();
    let script_server = tokio::spawn(async move {
        let cases = [
            ("/default.js", Some(full_referer.clone())),
            ("/empty.js", Some(full_referer.clone())),
            ("/origin.js", Some(cross_origin_referer.clone())),
            ("/invalid.js", Some(full_referer)),
            ("/none.js", None),
            ("/redirect.js", None),
            ("/redirect-target.js", Some(cross_origin_referer.clone())),
        ];
        for (expected_path, expected_referer) in cases {
            let (mut stream, _) = script_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            assert_eq!(
                request_header(&request, "referer"),
                expected_referer,
                "{expected_path}"
            );
            if expected_path == "/redirect.js" {
                stream
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: /redirect-target.js\r\nReferrer-Policy: origin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            } else {
                stream
                    .write_all(script_response(&script_body, "no-store").as_bytes())
                    .await
                    .unwrap();
            }
        }

        let (mut stream, _) = script_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/dynamic-origin.js")
        );
        assert_eq!(
            request_header(&request, "referer").as_deref(),
            Some(cross_origin_referer.as_str())
        );
        stream
            .write_all(script_response(&script_body, "no-store").as_bytes())
            .await
            .unwrap();
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(document_url)).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const origin = document.getElementById('origin'); const invalid = document.getElementById('invalid'); return [origin.referrerPolicy, invalid.referrerPolicy, invalid.getAttribute('referrerpolicy')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["ORIGIN", "", "not-a-policy"])
    );
    assert_eq!(
        engine
            .evaluate_async(&format!(
                "(() => {{ const script = document.createElement('script'); script.src = 'http://{script_address}/dynamic-origin.js'; script.referrerPolicy = 'ORIGIN'; const reflected = [script.referrerPolicy, script.getAttribute('referrerpolicy')]; document.body.appendChild(script); return reflected; }})()"
            ))
            .await
            .unwrap(),
        serde_json::json!(["ORIGIN", "ORIGIN"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const script = document.createElement('script'); script.src = '/cache.js'; script.referrerPolicy = 'no-referrer'; document.body.appendChild(script); return script.referrerPolicy; })()",
            )
            .await
            .unwrap(),
        serde_json::json!("no-referrer")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const meta = document.createElement('meta'); meta.setAttribute('name', 'referrer'); meta.setAttribute('content', 'no-referrer'); document.head.appendChild(meta); const script = document.createElement('script'); script.src = '/dynamic-default.js'; document.body.appendChild(script); return [script.referrerPolicy, script.hasAttribute('referrerpolicy')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["", false])
    );
    assert_eq!(
        engine.evaluate_async("scriptReferrerLoads").await.unwrap(),
        serde_json::json!(11)
    );

    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    script_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_inherits_referrer_policy_through_static_module_graphs() {
    fn request_header(request: &str, name: &str) -> Option<String> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_owned())
            })
        })
    }

    fn module_response(body: &str, referrer_policy: Option<&str>) -> String {
        let referrer_policy_header = referrer_policy
            .map(|policy| format!("Referrer-Policy: {policy}\r\n"))
            .unwrap_or_default();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nAccess-Control-Allow-Origin: *\r\n{referrer_policy_header}Cache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let module_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let module_address = module_listener.local_addr().unwrap();
    let document_url = format!("http://{page_address}/page?secret=token#fragment");
    let document_referer = format!("http://{page_address}/page?secret=token");
    let document_origin = format!("http://{page_address}/");
    let module_origin = format!("http://{module_address}/");
    let module_child_url = format!("http://{module_address}/child.js");

    let page_server = tokio::spawn(async move {
        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/page?secret=token")
        );
        let body = format!(
            "<script>const dynamicModule = document.createElement('script'); dynamicModule.type = 'module'; dynamicModule.referrerPolicy = 'origin'; dynamicModule.src = 'http://{module_address}/dynamic.js'; document.head.appendChild(dynamicModule);</script>\
             <script id='entry' type='module' src='http://{module_address}/entry.js' referrerpolicy='unsafe-url'></script>\
             <script id='inline' type='module' referrerpolicy='no-referrer'>import 'http://{module_address}/inline.js';</script>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nReferrer-Policy: no-referrer\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let expected_document_referer = document_referer.clone();
    let expected_document_origin = document_origin.clone();
    let expected_module_origin = module_origin.clone();
    let expected_child_url = module_child_url.clone();
    let module_server = tokio::spawn(async move {
        for _ in 0..7 {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(30), module_listener.accept())
                    .await
                    .expect("module graph request must arrive")
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("module request must include a path");
            let (referer, body, policy) = match path {
                "/entry.js" => (
                    Some(expected_document_referer.as_str()),
                    "import './redirect.js'; globalThis.moduleEntryLoaded = true;",
                    Some("origin"),
                ),
                "/redirect.js" => (Some(expected_module_origin.as_str()), "", None),
                "/child.js" => (
                    None,
                    "import './leaf.js'; globalThis.moduleChildLoaded = true;",
                    Some("unsafe-url"),
                ),
                "/leaf.js" => (
                    Some(expected_child_url.as_str()),
                    "globalThis.moduleLeafLoaded = true;",
                    None,
                ),
                "/inline.js" => (None, "import './inline-leaf.js';", None),
                "/inline-leaf.js" => (None, "globalThis.inlineModuleLoaded = true;", None),
                "/dynamic.js" => (
                    Some(expected_document_origin.as_str()),
                    "globalThis.dynamicModuleLoaded = true;",
                    None,
                ),
                other => panic!("unexpected module graph request: {other}"),
            };
            assert_eq!(
                request_header(&request, "referer").as_deref(),
                referer,
                "{path}"
            );
            if path == "/redirect.js" {
                stream
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: /child.js\r\nAccess-Control-Allow-Origin: *\r\nReferrer-Policy: no-referrer\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            } else {
                stream
                    .write_all(module_response(body, policy).as_bytes())
                    .await
                    .unwrap();
            }
        }
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(document_url)).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "[globalThis.moduleEntryLoaded, globalThis.moduleChildLoaded, globalThis.moduleLeafLoaded, globalThis.inlineModuleLoaded, globalThis.dynamicModuleLoaded]",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, true, true, true])
    );

    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    module_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_inherits_referrer_policy_through_dynamic_module_graphs() {
    fn request_header(request: &str, name: &str) -> Option<String> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_owned())
            })
        })
    }

    fn module_response(body: &str, referrer_policy: Option<&str>) -> String {
        let referrer_policy_header = referrer_policy
            .map(|policy| format!("Referrer-Policy: {policy}\r\n"))
            .unwrap_or_default();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nAccess-Control-Allow-Origin: *\r\n{referrer_policy_header}Cache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let module_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let module_address = module_listener.local_addr().unwrap();
    let document_url = format!("http://{page_address}/page?secret=token#fragment");
    let document_referer = format!("http://{page_address}/page?secret=token");
    let document_origin = format!("http://{page_address}/");
    let module_origin = format!("http://{module_address}/");

    let expected_document_origin = document_origin.clone();
    let page_server = tokio::spawn(async move {
        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/page?secret=token")
        );
        let body = format!(
            "<script type='module' src='http://{module_address}/entry.js' referrerpolicy='unsafe-url'></script><script type='module' referrerpolicy='origin'>globalThis.inlineDynamicImportError = null; import('./inline-dynamic.js').then(() => {{ globalThis.inlineDynamicImportSettled = true; }}, error => {{ globalThis.inlineDynamicImportError = String(error); }});</script>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nReferrer-Policy: no-referrer\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        drop(stream);

        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), page_listener.accept())
            .await
            .expect("inline dynamic module request must arrive on the document origin")
            .unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/inline-dynamic.js")
        );
        assert_eq!(
            request_header(&request, "referer").as_deref(),
            Some(expected_document_origin.as_str()),
            "/inline-dynamic.js"
        );
        stream
            .write_all(
                module_response("globalThis.inlineDynamicModuleLoaded = true;", None).as_bytes(),
            )
            .await
            .unwrap();
    });

    let expected_document_referer = document_referer.clone();
    let expected_module_origin = module_origin.clone();
    let module_server = tokio::spawn(async move {
        for _ in 0..4 {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(30), module_listener.accept())
                    .await
                    .expect("dynamic module graph request must arrive")
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("module request must include a path");
            let (referer, body, policy) = match path {
                "/entry.js" => (
                    Some(expected_document_referer.as_str()),
                    "import('./dynamic.js').then(() => { globalThis.dynamicEntrySettled = true; });",
                    Some("no-referrer"),
                ),
                "/dynamic.js" => (
                    None,
                    "import './static-dep.js'; import('./nested.js').then(() => { globalThis.nestedImportSettled = true; }); globalThis.dynamicModuleEvaluated = true;",
                    Some("origin"),
                ),
                "/static-dep.js" => (
                    Some(expected_module_origin.as_str()),
                    "globalThis.staticModuleDependencyLoaded = true;",
                    None,
                ),
                "/nested.js" => (
                    Some(expected_module_origin.as_str()),
                    "globalThis.nestedDynamicModuleLoaded = true;",
                    None,
                ),
                other => panic!("unexpected dynamic module graph request: {other}"),
            };
            assert_eq!(
                request_header(&request, "referer").as_deref(),
                referer,
                "{path}"
            );
            stream
                .write_all(module_response(body, policy).as_bytes())
                .await
                .unwrap();
        }
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(document_url)).unwrap();
    engine.initialize_async().await.unwrap();
    let result = engine
        .evaluate_async(
            "[globalThis.dynamicEntrySettled, globalThis.dynamicModuleEvaluated, globalThis.staticModuleDependencyLoaded, globalThis.nestedDynamicModuleLoaded, globalThis.nestedImportSettled, globalThis.inlineDynamicModuleLoaded, globalThis.inlineDynamicImportSettled, globalThis.inlineDynamicImportError, globalThis.__glassFetchRequests.size]",
        )
        .await
        .unwrap();
    page_server.await.unwrap();
    module_server.await.unwrap();
    engine.close_async().await.unwrap();
    assert_eq!(
        result,
        serde_json::json!([true, true, true, true, true, true, true, null, 0])
    );
}
#[tokio::test]
async fn native_content_process_loads_blob_object_url_media_subresources() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<audio id='audio'></audio><script>globalThis.mediaEvents = []; const audio = document.getElementById('audio'); audio.addEventListener('load', () => mediaEvents.push('load')); audio.addEventListener('error', () => mediaEvents.push('error'));</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'; media-src blob:\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const audio = document.getElementById('audio'); const url = URL.createObjectURL(new Blob([new Uint8Array([0x1a, 0x45, 0xdf, 0xa3])], { type: 'audio/webm' })); audio.src = url; return url; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(format!("blob:http://{address}/glass-native-1"))
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const audio = document.getElementById('audio'); return [mediaEvents, audio.readyState, audio.networkState, audio.currentSrc, audio.error, audio.canPlayType('audio/webm')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            ["load"],
            1,
            1,
            format!("blob:http://{address}/glass-native-1"),
            serde_json::Value::Null,
            "maybe"
        ])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_blocks_csp_disallowed_blob_media_subresources() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<audio id='audio'></audio><script>globalThis.mediaError = 0; const audio = document.getElementById('audio'); audio.addEventListener('error', () => mediaError += 1);</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'; media-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const audio = document.getElementById('audio'); const url = URL.createObjectURL(new Blob([new Uint8Array([0x1a, 0x45, 0xdf, 0xa3])], { type: 'audio/webm' })); audio.src = url; return url; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(format!("blob:http://{address}/glass-native-1"))
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const audio = document.getElementById('audio'); return [mediaError, audio.readyState, audio.networkState, audio.currentSrc, audio.error && audio.error.code]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            1,
            0,
            3,
            format!("blob:http://{address}/glass-native-1"),
            4
        ])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_static_http_media_subresource() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/media.webm"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/media.webm" {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: audio/webm\r\nContent-Length: 4\r\nConnection: close\r\n\r\n\x1a\x45\xdf\xa3",
                    )
                    .await
                    .unwrap();
            } else {
                let body = "<audio id='audio' src='/media.webm'></audio><script>globalThis.mediaEvents = []; const audio = document.getElementById('audio'); audio.addEventListener('load', () => mediaEvents.push('load')); audio.addEventListener('error', () => mediaEvents.push('error'));</script>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'; media-src 'self'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const audio = document.getElementById('audio'); return [mediaEvents, audio.readyState, audio.networkState, audio.currentSrc, audio.error, audio.canPlayType('audio/webm')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            ["load"],
            1,
            1,
            format!("http://{address}/media.webm"),
            serde_json::Value::Null,
            "maybe"
        ])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_blob_object_url_classic_script_subresources() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body><div id='host'>Blob script</div><script>globalThis.blobScriptRan = 0; globalThis.blobScriptLoaded = false; const script = document.createElement('script'); const url = URL.createObjectURL(new Blob(['globalThis.blobScriptRan += 1;'], { type: 'text/javascript' })); script.src = url; script.addEventListener('load', () => { globalThis.blobScriptLoaded = true; }); document.getElementById('host').appendChild(script); globalThis.blobScriptUrl = url;</script></body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline' blob:\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "[globalThis.blobScriptRan, globalThis.blobScriptLoaded, globalThis.blobScriptUrl]"
            )
            .await
            .unwrap(),
        serde_json::json!([1, true, format!("blob:http://{address}/glass-native-1")])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_blob_object_url_module_dependencies() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<head></head><body><main>Blob module</main></body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src blob:\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { globalThis.blobModuleLoaded = false; const dependency = URL.createObjectURL(new Blob(['export const value = 7;'], { type: 'text/javascript' })); const source = \"import { value } from '\" + dependency + \"'; globalThis.blobModuleValue = value;\"; const root = URL.createObjectURL(new Blob([source], { type: 'text/javascript' })); const script = document.createElement('script'); script.type = 'module'; script.src = root; script.addEventListener('load', () => { globalThis.blobModuleLoaded = true; }); document.head.appendChild(script); globalThis.blobModuleRoot = root; return root; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(format!("blob:http://{address}/glass-native-2"))
    );
    assert_eq!(
        engine
            .evaluate_async("[globalThis.blobModuleValue, globalThis.blobModuleLoaded]")
            .await
            .unwrap(),
        serde_json::json!([7, true])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_revalidates_no_cache_external_png_with_etag() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for request_index in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            if request_index == 0 {
                assert_eq!(request.split_whitespace().nth(1), Some("/page"));
                let body = "<style>body{margin:0px}</style><div style='width:16px'><img id='first' src='/image.png' width='8'><img id='second' src='/image.png' width='8'></div>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            } else {
                assert_eq!(request.split_whitespace().nth(1), Some("/image.png"));
                if request_index == 1 {
                    assert!(
                        !request
                            .lines()
                            .any(|line| line.eq_ignore_ascii_case("if-none-match: \"v1\""))
                    );
                    let headers = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: no-cache\r\nETag: \"v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        png.len()
                    );
                    stream.write_all(headers.as_bytes()).await.unwrap();
                    stream.write_all(&png).await.unwrap();
                } else {
                    assert!(
                        request
                            .lines()
                            .any(|line| line.eq_ignore_ascii_case("if-none-match: \"v1\""))
                    );
                    stream
                        .write_all(
                            b"HTTP/1.1 304 Not Modified\r\nCache-Control: no-cache\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n",
                        )
                        .await
                        .unwrap();
                }
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 16,
                height: 8,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .filter(|command| matches!(command, NativeDisplayCommand::Image { .. }))
            .count(),
        2
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_merges_stale_profile_deltas() {
    let _guard = native_content_process_test_lock().lock().await;
    let file_name = format!(
        "glass-native-web-storage-{}-content-stale-merge.json",
        std::process::id()
    );
    let first_path = std::env::temp_dir().join(&file_name);
    let second_path = std::env::temp_dir().join(".").join(&file_name);
    let lock_path = first_path.with_extension("lock");
    let _ = fs::remove_file(&first_path);
    let _ = fs::remove_file(&lock_path);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some("/page"));
            let body = "<p>Content stale profile merge</p>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let url = format!("http://{address}/page");
    let first_config = NativeEngineConfig::default()
        .with_storage_path(first_path.clone())
        .with_initial_url(url.clone());
    let second_config = first_config.clone().with_storage_path(second_path);
    let mut first = NativeEngine::new(first_config).unwrap();
    let mut second = NativeEngine::new(second_config).unwrap();
    first.initialize_async().await.unwrap();
    second.initialize_async().await.unwrap();

    first
        .evaluate_async("localStorage.setItem('first', 'one'); true")
        .await
        .unwrap();
    second
        .evaluate_async("localStorage.setItem('second', 'two'); true")
        .await
        .unwrap();
    second.close_async().await.unwrap();
    first.close_async().await.unwrap();
    server.await.unwrap();

    let profile: serde_json::Value =
        serde_json::from_slice(&fs::read(&first_path).unwrap()).unwrap();
    let local = profile["local"].as_object().unwrap();
    assert_eq!(local.len(), 1);
    let entries = local.values().next().unwrap();
    assert_eq!(entries["first"], "one");
    assert_eq!(entries["second"], "two");

    let _ = fs::remove_file(first_path);
    let _ = fs::remove_file(lock_path);
}
#[tokio::test]
async fn native_content_process_tracks_request_body_disturbance_and_clone_ownership() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (index, expected_path) in ["/page", "/source", "/source", "/source"]
            .into_iter()
            .enumerate()
        {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path != "/page" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                let expected_body = if index == 2 { "override" } else { "source" };
                assert_eq!(body, expected_body);
            }
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<body>Request body owner</body>")
            } else {
                ("text/plain", "ok")
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = new Request('/source', { method: 'POST', body: 'source' }); const clone = request.clone(); const initial = [request.bodyUsed, clone.bodyUsed]; const first = await fetch(request); const firstValue = await first.text(); const afterFirst = [firstValue, request.bodyUsed, clone.bodyUsed]; const reused = await fetch(request).then(() => 'readable', error => error.name); const constructedFromUsed = (() => { try { new Request(request); return 'constructable'; } catch (error) { return error.name; } })(); const overridden = await fetch(request, { body: 'override' }); const overrideValue = await overridden.text(); const cloned = await fetch(clone); const cloneValue = await cloned.text(); const cloneAfter = [cloneValue, clone.bodyUsed, (() => { try { clone.clone(); return 'cloneable'; } catch (error) { return error.name; } })()]; return { initial, afterFirst, reused, constructedFromUsed, overrideValue, cloneAfter }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "initial": [false, false],
            "afterFirst": ["ok", true, false],
            "reused": "TypeError",
            "constructedFromUsed": "TypeError",
            "overrideValue": "ok",
            "cloneAfter": ["ok", true, "TypeError"]
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_exposes_request_body_stream_and_methods() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>Request body surface</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const textRequest = new Request('/text', { method: 'POST', body: 'hello' }); const textInitial = [textRequest.body instanceof ReadableStream, textRequest.bodyUsed, textRequest.body.locked]; const text = await textRequest.text(); const textReader = textRequest.body.getReader(); const textAfterRead = await textReader.read().then(() => 'readable', error => error.name); textReader.releaseLock(); const textAfter = [text, textRequest.bodyUsed, textAfterRead]; const jsonRequest = new Request('/json', { method: 'POST', body: '{\"ok\":true}' }); const json = await jsonRequest.json(); const binaryRequest = new Request('/binary', { method: 'POST', body: new Uint8Array([0, 255, 128, 70]) }); const buffer = await binaryRequest.arrayBuffer(); const bytesRequest = new Request('/bytes', { method: 'POST', body: new Uint8Array([9, 8, 7]) }); const bytes = await bytesRequest.bytes(); const blobRequest = new Request('/blob', { method: 'POST', body: new Blob(['blob-body'], { type: 'text/custom' }) }); const blob = await blobRequest.blob(); const cloneSource = new Request('/clone', { method: 'POST', body: 'independent' }); const clone = cloneSource.clone(); const cloneText = await clone.text(); const cloneState = [cloneText, cloneSource.bodyUsed, clone.bodyUsed]; const streamRequest = new Request('/stream', { method: 'POST', body: 'streamed' }); const reader = streamRequest.body.getReader(); const first = await reader.read(); const end = await reader.read(); reader.releaseLock(); const streamState = [Array.from(first.value), first.done, end.value, end.done, streamRequest.bodyUsed, streamRequest.body.locked]; const lockedRequest = new Request('/locked', { method: 'POST', body: 'locked' }); const lockedReader = lockedRequest.body.getReader(); const lockedClone = (() => { try { lockedRequest.clone(); return 'cloneable'; } catch (error) { return error.name; } })(); lockedReader.releaseLock(); const emptyRequest = new Request('/empty'); const empty = await emptyRequest.text(); return { textInitial, textAfter, json, buffer: Array.from(new Uint8Array(buffer)), bytes: Array.from(bytes), blob: [blob instanceof Blob, blob.type, blob.size, await blob.text()], cloneState, streamState, lockedClone, empty: [empty, emptyRequest.bodyUsed] }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "textInitial": [true, false, false],
            "textAfter": ["hello", true, "TypeError"],
            "json": {"ok": true},
            "buffer": [0, 255, 128, 70],
            "bytes": [9, 8, 7],
            "blob": [true, "text/custom", 9, "blob-body"],
            "cloneState": ["independent", false, true],
            "streamState": [[115, 116, 114, 101, 97, 109, 101, 100], false, null, true, true, false],
            "lockedClone": "TypeError",
            "empty": ["", true]
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_drives_underlying_readable_stream_sources() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>ReadableStream source</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const events = []; const stream = new ReadableStream({ start(controller) { events.push('start'); controller.enqueue('first'); }, pull(controller) { events.push('pull'); controller.enqueue('second'); controller.close(); }, cancel(reason) { events.push('cancel:' + reason); } }); const reader = stream.getReader(); const first = await reader.read(); const second = await reader.read(); const end = await reader.read(); const beforeRelease = stream.locked; reader.releaseLock(); const canceled = new ReadableStream({ cancel(reason) { events.push('cancel:' + reason); } }); await canceled.cancel('stop'); return { events, first, second, end: [end.value, end.done], beforeRelease, afterRelease: stream.locked, canceled: canceled.locked }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["start", "pull", "cancel:stop"],
            "first": {"value": "first", "done": false},
            "second": {"value": "second", "done": false},
            "end": [null, true],
            "beforeRelease": true,
            "afterRelease": false,
            "canceled": false
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_resolves_readable_stream_reader_closed() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>ReadableStream reader closed</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { let controller; const stream = new ReadableStream({ start(candidate) { controller = candidate; candidate.enqueue('value'); } }); const reader = stream.getReader(); const closed = reader.closed.then(() => 'resolved', error => 'rejected:' + error.name); const first = await reader.read(); reader.releaseLock(); controller.close(); const replacement = stream.getReader(); const replacementClosed = replacement.closed.then(() => 'resolved', error => 'rejected:' + error.name); const end = await replacement.read(); replacement.releaseLock(); let errorController; const errored = new ReadableStream({ start(candidate) { errorController = candidate; } }); const errorReader = errored.getReader(); const errorClosed = errorReader.closed.then(() => 'resolved', error => 'rejected:' + error.name); errorController.error(new Error('boom')); const errorRead = await errorReader.read().then(() => 'readable', error => error.name); let cancelEvents = []; const canceled = new ReadableStream({ cancel(reason) { cancelEvents.push('cancel:' + reason); } }); const canceledReader = canceled.getReader(); const canceledClosed = canceledReader.closed.then(() => 'resolved', error => 'rejected:' + error.name); await canceledReader.cancel('stop'); canceledReader.releaseLock(); return { first, closed: await closed, replacementClosed: await replacementClosed, end: [end.value, end.done], errorClosed: await errorClosed, errorRead, cancelEvents, canceledClosed: await canceledClosed }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "first": {"value": "value", "done": false},
            "closed": "resolved",
            "replacementClosed": "resolved",
            "end": [null, true],
            "errorClosed": "rejected:Error",
            "errorRead": "Error",
            "cancelEvents": ["cancel:stop"],
            "canceledClosed": "resolved"
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_tees_underlying_readable_stream_sources() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>ReadableStream tee</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const events = []; let pulls = 0; const source = new ReadableStream({ start(controller) { events.push('start'); controller.enqueue('a'); }, pull(controller) { events.push('pull'); if (pulls++ === 0) { controller.enqueue('b'); controller.close(); } }, cancel(reason) { events.push('cancel:' + reason); } }); const [left, right] = source.tee(); const leftReader = left.getReader(); const rightReader = right.getReader(); const leftFirst = await leftReader.read(); const rightFirst = await rightReader.read(); const leftSecond = await leftReader.read(); const rightSecond = await rightReader.read(); const leftEnd = await leftReader.read(); const rightEnd = await rightReader.read(); leftReader.releaseLock(); rightReader.releaseLock(); return { events, left: [leftFirst.value, leftSecond.value, leftEnd.done], right: [rightFirst.value, rightSecond.value, rightEnd.done], sourceLocked: source.locked, leftLocked: left.locked, rightLocked: right.locked }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["start", "pull"],
            "left": ["a", "b", true],
            "right": ["a", "b", true],
            "sourceLocked": false,
            "leftLocked": false,
            "rightLocked": false
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_cancels_tee_upstream_after_both_branches_cancel() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>ReadableStream tee cancel</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const events = []; const source = new ReadableStream({ pull(controller) { controller.enqueue('value'); }, cancel(reason) { events.push('cancel:' + reason); } }); const [left, right] = source.tee(); const leftReader = left.getReader(); const rightReader = right.getReader(); await leftReader.cancel('left'); const rightValue = await rightReader.read(); await rightReader.cancel('right'); leftReader.releaseLock(); rightReader.releaseLock(); return { rightValue, events, sourceLocked: source.locked, leftLocked: left.locked, rightLocked: right.locked }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "rightValue": {"value": "value", "done": false},
            "events": ["cancel:right"],
            "sourceLocked": false,
            "leftLocked": false,
            "rightLocked": false
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_exposes_live_template_content_fragments() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("native page request")
            .unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<!doctype html><template id='template'><span id='inside'>inert</span><template id='nested'><b>nested</b></template></template><div id='root'></div>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const template = document.querySelector('#template');
                    const content = template.content;
                    const original = content.querySelector('#inside');
                    const nested = content.querySelector('#nested');
                    const root = document.querySelector('#root');
                    const initial = {
                      templateType: template instanceof HTMLTemplateElement,
                      contentIdentity: content === template.content,
                      fragmentType: content instanceof DocumentFragment,
                      fragmentParent: content.parentNode === null,
                      childParent: original.parentNode === content,
                      childParentElement: original.parentElement === null,
                      rootIdentity: original.getRootNode() === content,
                      ownerDocumentType: content.ownerDocument instanceof Document,
                      ownerDocumentDetached: content.ownerDocument !== document,
                      nestedOwnerDocument: nested.content.ownerDocument === content.ownerDocument,
                      templateChildCount: template.childNodes.length === 0,
                      templateQuery: template.querySelector('#inside') === null,
                      documentQuery: document.querySelector('#inside') === null,
                      innerHTML: template.innerHTML.includes('id="inside"'),
                    };
                    const shallowClone = template.cloneNode(false);
                    const deepClone = template.cloneNode(true);
                    const shallowContent = shallowClone.content;
                    const deepContent = deepClone.content;
                    const clonedInside = deepContent.querySelector('#inside');
                    const clonedNested = deepContent.querySelector('#nested');
                    const clonedNestedChild = clonedNested.content.querySelector('b');
                    const clones = {
                      shallowFragmentIsDistinct: shallowContent !== content,
                      shallowFragmentIsStable: shallowContent === shallowClone.content,
                      shallowFragmentIsEmpty: shallowContent.childNodes.length === 0,
                      shallowOwnerIsInert: shallowContent.ownerDocument === content.ownerDocument,
                      deepFragmentIsDistinct: deepContent !== content,
                      deepFragmentOwnerIsInert: deepContent.ownerDocument === content.ownerDocument,
                      deepChildIsDistinct: clonedInside !== original,
                      deepChildParent: clonedInside.parentNode === deepContent,
                      deepOwnerIsInert: clonedInside.ownerDocument === content.ownerDocument,
                      nestedTemplateIsDistinct: clonedNested !== nested,
                      nestedContentIsDistinct: clonedNested.content !== nested.content,
                      nestedChildIsDistinct: clonedNestedChild !== nested.content.querySelector('b'),
                      nestedChildOwnerIsInert: clonedNestedChild.ownerDocument === content.ownerDocument,
                      templateBoundary: deepClone.childNodes.length === 0
                        && deepClone.querySelector('#inside') === null,
                      nestedTemplateBoundary: deepContent.querySelector('b') === null,
                      sourceUnchanged: original.textContent === 'inert',
                    };
                    shallowClone.id = 'shallowClone';
                    deepClone.id = 'deepClone';
                    root.appendChild(deepClone);
                    root.appendChild(shallowClone);
                    const added = document.createElement('b');
                    added.id = 'added';
                    added.textContent = 'added';
                    content.appendChild(added);
                    root.appendChild(original);
                    const moved = content.querySelector('#added') === added
                      && original.parentNode === root
                      && document.querySelector('#inside') === original;
                    template.innerHTML = '<em id="replacement">new</em>';
                    globalThis.__templateRef = template;
                    globalThis.__templateContentRef = content;
                    return { initial, clones, moved, replacement: content.querySelector('#replacement') !== null };
                  })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "initial": {
                "templateType": true,
                "contentIdentity": true,
                "fragmentType": true,
                "fragmentParent": true,
                "childParent": true,
                "childParentElement": true,
                "rootIdentity": true,
                "ownerDocumentType": true,
                "ownerDocumentDetached": true,
                "nestedOwnerDocument": true,
                "templateChildCount": true,
                "templateQuery": true,
                "documentQuery": true,
                "innerHTML": true,
            },
            "clones": {
                "shallowFragmentIsDistinct": true,
                "shallowFragmentIsStable": true,
                "shallowFragmentIsEmpty": true,
                "shallowOwnerIsInert": true,
                "deepFragmentIsDistinct": true,
                "deepFragmentOwnerIsInert": true,
                "deepChildIsDistinct": true,
                "deepChildParent": true,
                "deepOwnerIsInert": true,
                "nestedTemplateIsDistinct": true,
                "nestedContentIsDistinct": true,
                "nestedChildIsDistinct": true,
                "nestedChildOwnerIsInert": true,
                "templateBoundary": true,
                "nestedTemplateBoundary": true,
                "sourceUnchanged": true,
            },
            "moved": true,
            "replacement": true,
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const template = globalThis.__templateRef;
                    const content = globalThis.__templateContentRef;
                    return {
                      stable: template.content === content,
                      replacement: content.querySelector('#replacement')?.textContent,
                      templateQuery: template.querySelector('#replacement') === null,
                      documentQuery: document.querySelector('#replacement') === null,
                      movedNode: document.querySelector('#inside')?.textContent,
                      deepCloneContent: document.querySelector('#deepClone')?.content.querySelector('#inside')?.textContent,
                      deepCloneNested: document.querySelector('#deepClone')?.content.querySelector('#nested')?.content.querySelector('b')?.textContent,
                      shallowCloneEmpty: document.querySelector('#shallowClone')?.content.childNodes.length === 0,
                    };
                  })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "stable": true,
            "replacement": "new",
            "templateQuery": true,
            "documentQuery": true,
            "movedNode": "inert",
            "deepCloneContent": "inert",
            "deepCloneNested": "nested",
            "shallowCloneEmpty": true,
        })
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}

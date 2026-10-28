use super::*;

#[test]
fn native_local_script_exposes_web_idl_identity_and_dom_collections() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_local_script_exposes_web_idl_identity_and_dom_collections_inner());
    });
}

async fn native_local_script_exposes_web_idl_identity_and_dom_collections_inner() {
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Chtml%3E%3Cbody%3E%3Cform%3E%3Cinput%20id%3D%27name%27%3E%3C%2Fform%3E%3C%2Fbody%3E%3C%2Fhtml%3E",
        ),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let identity = engine
        .evaluate_async(
            "(() => { const input = document.getElementById('name'); const event = new Event('click', { bubbles: true }); const custom = new CustomEvent('custom', { detail: { ok: true } }); const storage = new StorageEvent('storage', { key: 'theme', newValue: 'dark' }); const nodes = document.querySelectorAll('input'); const tags = document.getElementsByTagName('input'); const text = document.createTextNode('native'); return { window: window === globalThis && window === self && window instanceof Window, document: document instanceof Document && document.defaultView === window && document.nodeType === 9 && document.nodeName === '#document', input: input instanceof Node && input instanceof Element && input instanceof HTMLElement && input instanceof HTMLInputElement && input.nodeType === 1 && input.nodeName === 'INPUT' && input.localName === 'input' && input.ownerDocument === document, body: document.body instanceof HTMLBodyElement, collections: [nodes instanceof NodeList, tags instanceof HTMLCollection, Array.isArray(nodes), Array.isArray(tags)], location: location instanceof Location, events: [event instanceof Event, custom instanceof Event, custom instanceof CustomEvent, custom.detail.ok, storage instanceof Event, storage instanceof StorageEvent, storage.key, storage.newValue], text: [text instanceof Node, text instanceof CharacterData, text instanceof Text, text.data, text.nodeValue, text.length] }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        identity,
        serde_json::json!({
            "window": true,
            "document": true,
            "input": true,
            "body": true,
            "collections": [true, true, true, true],
            "location": true,
            "events": [true, true, true, true, true, true, "theme", "dark"],
            "text": [true, true, true, "native", "native", 6],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const host = document.createElement('div'); host.setAttribute('data-existing', 'yes'); const attr = document.createAttribute('DATA-OWNED'); attr.value = 'one'; const identity = [attr instanceof Node, attr instanceof Attr, attr.nodeType, attr.nodeName, attr.name, attr.localName, attr.value, attr.nodeValue, attr.textContent, attr.ownerElement === null, attr.ownerDocument === document, attr.parentNode === null]; const first = host.setAttributeNode(attr); const attributes = host.attributes; const attached = [first === null, attributes instanceof NamedNodeMap, attributes.length, attributes[0].name, attributes[1] === attr, attributes.getNamedItem('data-owned') === attr, attr.ownerElement === host, host.getAttribute('data-owned') === 'one', document.querySelector('form').attributes instanceof NamedNodeMap]; attr.nodeValue = 'two'; const clone = attr.cloneNode(); const cloneState = [clone !== attr, clone instanceof Attr, clone.name, clone.value, clone.ownerElement === null, clone.parentNode === null, clone.getRootNode() === clone, clone.childNodes instanceof NodeList, clone.childNodes.length, clone.isEqualNode(attr)]; const changed = [attr.value, attr.textContent, host.getAttribute('data-owned')]; const replacement = document.createAttribute('data-owned'); replacement.value = 'three'; const old = host.setAttributeNode(replacement); const replaced = [old === attr, attr.ownerElement === null, replacement.ownerElement === host, attributes.getNamedItemNS(null, 'data-owned') === replacement, attributes.item(1) === replacement]; const mapped = document.createAttribute('data-mapped'); mapped.value = 'map'; const mapOld = attributes.setNamedItem(mapped); const mapState = [mapOld === null, attributes.length, attributes[2] === mapped, attributes.getNamedItem('data-mapped') === mapped, attributes.removeNamedItem('data-mapped') === mapped, mapped.ownerElement === null]; const removed = host.removeAttributeNode(replacement); const errors = [removed === replacement, replacement.ownerElement === null, host.getAttribute('data-owned') === null, (() => { try { document.createAttribute('bad name'); return null; } catch (error) { return error.name; } })(), (() => { try { host.removeAttributeNode(replacement); return null; } catch (error) { return error.name; } })()]; return { identity, attached, cloneState, changed, replaced, mapState, errors }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "identity": [true, true, 2, "data-owned", "data-owned", "data-owned", "one", "one", "one", true, true, true],
            "attached": [true, true, 2, "data-existing", true, true, true, true, true],
            "cloneState": [true, true, "data-owned", "two", true, true, true, true, 0, true],
            "changed": ["two", "two", "two"],
            "replaced": [true, true, true, true, true],
            "mapState": [true, 3, true, true, true, true],
            "errors": [true, true, true, "InvalidCharacterError", "NotFoundError"],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const text = document.createTextNode('abc'); const before = [text.data, text.length, text.substringData(1, 2)]; text.nodeValue = 'abcd'; text.insertData(1, 'X'); text.deleteData(2, 1); text.replaceData(1, 1, 'Y'); return [before, text.data, text.nodeValue, text.textContent, text.length, text.substringData(1, 2)]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([["abc", 3, "bc"], "aYcd", "aYcd", "aYcd", 4, "Yc"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const forms = document.forms; const extra = document.createElement('form'); extra.setAttribute('name', 'extra'); document.body.appendChild(extra); const result = [forms instanceof HTMLCollection, forms.length, forms[1] === extra, document.getElementsByName('extra')[0] === extra, document.getElementsByTagName('form').length, document.scrollingElement === document.documentElement]; extra.remove(); return [result, forms.length]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([[true, 2, true, true, 2, true], 1])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const before = document.title; document.title = 'Native title'; return [before, document.title, document.head !== null, document.querySelector('title').textContent]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["", "Native title", true, "Native title"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[document.title, document.head.tagName, document.head.firstElementChild.tagName]"
            )
            .await
            .unwrap(),
        serde_json::json!(["Native title", "HEAD", "TITLE"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const form = document.querySelector('form'); const input = document.getElementById('name'); const shallow = form.cloneNode(); const deep = form.cloneNode(true); return [shallow !== form, shallow.isEqualNode(form), shallow.childNodes.length, deep !== form, deep.isEqualNode(form), deep.firstElementChild !== input, deep.firstElementChild.isEqualNode(input), form.isSameNode(form), form.compareDocumentPosition(input), input.compareDocumentPosition(form), deep.parentNode === null, deep.getRootNode() === deep]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, false, 0, true, true, true, true, true, 20, 10, true, true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const host = document.createElement('section'); const first = document.createTextNode('a'); const empty = document.createTextNode(''); const second = document.createTextNode('b'); host.append(first, empty, second); host.normalize(); return [host.childNodes.length, host.firstChild.data, empty.parentNode === null, second.parentNode === null]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([1, "ab", true, true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const host = document.createElement('article'); const child = document.createElement('span'); child.id = 'persistent'; const text = document.createTextNode('persistent text'); host.append(child, text); document.body.appendChild(host); globalThis.__persistentHost = host; globalThis.__persistentChild = child; globalThis.__persistentText = text; return [child.isConnected, child.parentNode === host, text.parentNode === host]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const host = globalThis.__persistentHost; const child = globalThis.__persistentChild; const text = globalThis.__persistentText; const current = document.getElementById('persistent'); const currentText = host.lastChild; const result = [current === child, host.contains(child), child.getRootNode() === document, child.parentElement === host, currentText === text, host.contains(text), text.parentNode === host, text.getRootNode() === document, text.data]; text.data = 'updated'; host.remove(); return result; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, true, true, true, true, true, true, "persistent text"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const input = document.getElementById('name'); const detached = document.createElement('aside'); return [input.getRootNode() === document, input.isConnected, detached.getRootNode() === detached, detached.isConnected, input.outerHTML]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, true, false, "<input id=\"name\">"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const input = document.getElementById('name'); input.outerHTML = \"<textarea id='replacement'>updated</textarea>\"; const replacement = document.getElementById('replacement'); return [replacement.tagName, replacement.textContent, replacement.parentElement.tagName, document.getElementById('name')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["TEXTAREA", "updated", "FORM", null])
    );
    let created = engine
        .evaluate_async(
            "(() => { const host = document.createElement('section'); host.setAttribute('id', 'created'); const child = document.createElement('strong'); const text = document.createTextNode('created'); text.textContent = 'created text'; child.appendChild(text); host.appendChild(child); document.body.appendChild(host); const inserted = document.createElement('em'); inserted.textContent = 'inserted'; document.body.insertBefore(inserted, host); return [host.tagName, host.parentElement === document.body, child.parentElement === host, text.parentNode === child, host.textContent, host.innerHTML, document.body.__glassChildren.map(element => element.tagName)]; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        created,
        serde_json::json!([
            "SECTION",
            true,
            true,
            true,
            "created text",
            "<strong>created text</strong>",
            ["FORM", "EM", "SECTION"],
        ])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[document.getElementById('created').innerHTML, document.getElementById('created').textContent, document.querySelector('em').textContent]",
            )
            .await
            .unwrap(),
        serde_json::json!(["<strong>created text</strong>", "created text", "inserted"])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_script_exposes_comments_and_doctype_nodes() {
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://comments",
                "<!doctype html><!--before--><html><body><!--inside-->Visible</body></html>",
            )
            .unwrap()
            .with_initial_url("fixture://comments"),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(engine.snapshot().unwrap().visible_text, "Visible");
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const doctype = document.doctype; const rootComment = document.childNodes[1]; const bodyComment = document.body.firstChild; globalThis.__doctype = doctype; globalThis.__rootComment = rootComment; globalThis.__bodyComment = bodyComment; bodyComment.data = 'inside-updated'; return { doctype: [doctype instanceof Node, doctype instanceof DocumentType, doctype.nodeType, doctype.name, doctype.nodeName, doctype.nodeValue, doctype.parentNode === document, doctype.nextSibling === rootComment], rootComment: [rootComment instanceof Node, rootComment instanceof CharacterData, rootComment instanceof Comment, rootComment.nodeType, rootComment.nodeName, rootComment.data, rootComment.parentNode === document, rootComment.previousSibling === doctype, rootComment.nextSibling === document.documentElement], bodyComment: [bodyComment.nodeType, bodyComment.data, bodyComment.parentNode === document.body, bodyComment instanceof Comment, bodyComment.nextSibling === document.body.lastChild], visible: document.body.textContent, markup: document.body.innerHTML }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "doctype": [true, true, 10, "html", "html", null, true, true],
            "rootComment": [true, true, true, 8, "#comment", "before", true, true, true],
            "bodyComment": [8, "inside-updated", true, true, true],
            "visible": "Visible",
            "markup": "<!--inside-updated-->Visible",
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[globalThis.__doctype === document.doctype, globalThis.__doctype instanceof DocumentType, globalThis.__doctype.nextSibling === globalThis.__rootComment, globalThis.__rootComment === document.childNodes[1], globalThis.__rootComment instanceof Comment, globalThis.__rootComment.previousSibling === document.doctype, globalThis.__bodyComment === document.body.firstChild, globalThis.__bodyComment.data, globalThis.__bodyComment.nextSibling === document.body.lastChild, document.body.innerHTML]",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, true, true, true, true, true, "inside-updated", true, "<!--inside-updated-->Visible"])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_script_creates_and_persists_comment_nodes() {
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://created-comments",
                "<html><body><p>start</p></body></html>",
            )
            .unwrap()
            .with_initial_url("fixture://created-comments"),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const direct = document.createComment('direct'); const fragment = document.createDocumentFragment(); fragment.innerHTML = 'a<!--fragment--><b>b</b>'; document.body.append(direct, fragment); globalThis.__createdComment = direct; const nested = Array.from(document.body.childNodes).find(node => node.nodeType === 8 && node.data === 'fragment'); return { direct: [direct instanceof Node, direct instanceof CharacterData, direct instanceof Comment, direct.nodeType, direct.nodeName, direct.data, direct.parentNode === document.body, direct.parentElement === document.body], fragment: [fragment.childNodes.length, nested.nodeType, nested instanceof Comment, nested.data, nested.parentNode === document.body], content: [document.body.textContent, document.body.innerHTML], markup: direct.__glassMarkup }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "direct": [true, true, true, 8, "#comment", "direct", true, true],
            "fragment": [0, 8, true, "fragment", true],
            "content": ["startab", "<p>start</p><!--direct-->a<!--fragment--><b>b</b>"],
            "markup": "<!--direct-->",
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const current = Array.from(document.body.childNodes).find(node => node.nodeType === 8 && node.data === 'direct'); return [current === globalThis.__createdComment, globalThis.__createdComment.data, current.parentNode === document.body, current.previousSibling === document.body.firstChild, current.nextSibling === document.body.childNodes[2], document.body.textContent, document.body.innerHTML, Array.from(document.body.childNodes).map(node => [node.nodeName, node.textContent, node.innerHTML])]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, "direct", true, true, true, "startab", "<p>start</p><!--direct-->a<!--fragment--><b>b</b>", [["P", "start", "start"], ["#comment", "direct", null], ["#text", "a", null], ["#comment", "fragment", null], ["B", "b", "b"]]])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_script_creates_and_persists_document_types() {
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://created-doctype",
                "<html><body>doctype</body></html>",
            )
            .unwrap()
            .with_initial_url("fixture://created-doctype"),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const type = document.implementation.createDocumentType('html', 'public-id', 'system-id'); const clone = type.cloneNode(); document.insertBefore(type, document.documentElement); globalThis.__createdDocumentType = type; return { type: [document.implementation !== null, type instanceof Node, type instanceof DocumentType, type.nodeType, type.nodeName, type.name, type.nodeValue, type.textContent, type.publicId, type.systemId, type.parentNode === document, type.parentElement === null, document.doctype === type, type.__glassMarkup], clone: [clone !== type, clone instanceof DocumentType, clone.isEqualNode(type), clone.parentNode === null] }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "type": [true, true, true, 10, "html", "html", null, null, "public-id", "system-id", true, true, true, "<!DOCTYPE html PUBLIC \"public-id\" \"system-id\">"] ,
            "clone": [true, true, true, true],
        })
    );
    let persisted = engine
        .evaluate_async(
            "(() => { const current = document.doctype; const root = document.documentElement; return [current === globalThis.__createdDocumentType, current.name, current.publicId, current.systemId, current.parentNode === document, root.__glassParent === document, root.previousSibling === current]; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        persisted,
        serde_json::json!([true, "html", "public-id", "system-id", true, true, true])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dynamic_inline_script_runs_once_after_late_attachment() {
    let mut engine = NativeEngine::new(NativeEngineConfig::default().with_initial_url(
        "data:text/html,%3Chtml%3E%3Cbody%3E%3Cp%3ENative%3C%2Fp%3E%3C%2Fbody%3E%3C%2Fhtml%3E",
    ))
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
                "document.body.appendChild(dynamicScript); [dynamicLog, document.body.getAttribute('data-dynamic'), dynamicScript.__glassDynamicScriptStarted, dynamicScript.textContent, dynamicScript.parentNode === document.body, document.body.parentNode === document.documentElement]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            ["ran"],
            "yes",
            true,
            "dynamicLog.push('ran'); document.body.setAttribute('data-dynamic', 'yes');",
            true,
            true
        ])
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
}
#[tokio::test]
async fn native_local_dynamic_blob_classic_script_runs_after_late_attachment() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://blob-dynamic-script",
            "<html><body><p>Native</p></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://blob-dynamic-script");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { globalThis.localBlobLog = []; globalThis.localBlobScript = document.createElement('script'); const url = URL.createObjectURL(new Blob([\"localBlobLog.push('ran'); document.body.setAttribute('data-local-blob', 'yes');\"], { type: 'text/javascript' })); localBlobScript.src = url; localBlobScript.addEventListener('load', () => localBlobLog.push('loaded')); document.body.appendChild(localBlobScript); return url; })()",
            )
            .await
            .unwrap(),
        serde_json::json!("blob:null/glass-native-1")
    );
    assert_eq!(
        engine
            .evaluate_async("[localBlobLog, document.body.getAttribute('data-local-blob')]",)
            .await
            .unwrap(),
        serde_json::json!([["ran", "loaded"], "yes"])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_script_exposes_computed_style_and_media_queries() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://computed-style",
            "<style>#target { display: flex; color: #102030; padding: 4px; margin-left: 3px; }</style><div id='target'>Native</div>",
        )
        .unwrap()
        .with_initial_url("fixture://computed-style");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const target = document.getElementById('target'); const style = getComputedStyle(target); const wide = matchMedia('(min-width: 600px) and (orientation: landscape)'); const narrow = matchMedia('(max-width: 599px)'); const before = [style.display, style.color, style.paddingTop, style.getPropertyValue('margin-left'), style.item(0), style.length > 0, wide.media, wide.matches, typeof wide.addEventListener, narrow.matches]; target.style.display = 'none'; return [before, getComputedStyle(target).display]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            [
                "flex",
                "rgb(16, 32, 48)",
                "4px",
                "3px",
                "display",
                true,
                "(min-width: 600px) and (orientation: landscape)",
                true,
                "function",
                false,
            ],
            "none",
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_inline_page_scripts_run_before_commit_and_persist_realm() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://inline-script-load",
            "<script>globalThis.inlineCount = (globalThis.inlineCount || 0) + 1;</script><script>globalThis.inlineCount += 1; document.getElementById('name').value = 'loaded';</script><input id='name' type='text'>",
        )
        .unwrap()
        .with_initial_url("fixture://inline-script-load");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(engine.revision(), 1);
    assert_eq!(
        engine
            .evaluate_async("globalThis.inlineCount")
            .await
            .unwrap(),
        serde_json::json!(2)
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('name').value")
            .await
            .unwrap(),
        serde_json::json!("loaded")
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_lifecycle_events_fire_after_script_schedule() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://page-lifecycle",
            "<script>globalThis.lifecycle = []; document.addEventListener('readystatechange', () => lifecycle.push('ready:' + document.readyState)); document.addEventListener('DOMContentLoaded', () => lifecycle.push('dom:' + document.readyState)); addEventListener('load', () => lifecycle.push('load:' + document.readyState));</script>",
        )
        .unwrap()
        .with_initial_url("fixture://page-lifecycle");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.lifecycle").await.unwrap(),
        serde_json::json!([
            "ready:interactive",
            "dom:interactive",
            "ready:complete",
            "load:complete"
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_pages_without_scripts_retain_complete_ready_state() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://no-scripts",
            "<title>No scripts</title><p>Ready</p>",
        )
        .unwrap()
        .with_initial_url("fixture://no-scripts");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("document.readyState").await.unwrap(),
        serde_json::json!("complete")
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_inline_modules_run_in_document_order() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://inline-module-load",
            "<script type='module'>globalThis.moduleOrder = 'first'; export const ready = true;</script><script type='module'>globalThis.moduleOrder += '-second';</script>",
        )
        .unwrap()
        .with_initial_url("fixture://inline-module-load");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async("globalThis.moduleOrder")
            .await
            .unwrap(),
        serde_json::json!("first-second")
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_scripts_honor_bounded_parser_timing_order() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://parser-timing",
            "<script>globalThis.parserOrder = 'blocking-1';</script><script>globalThis.parserOrder += '-blocking-2';</script><script type='module'>globalThis.parserOrder += '-module-defer';</script>",
        )
        .unwrap()
        .with_initial_url("fixture://parser-timing");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.parserOrder")
            .await
            .unwrap(),
        serde_json::json!("blocking-1-blocking-2-module-defer")
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_scripts_drain_microtasks_and_next_turn_timers() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://script-tasks",
            "<script>globalThis.taskOrder = 'script'; queueMicrotask(() => { globalThis.taskOrder += '-micro'; }); setTimeout(() => { globalThis.taskOrder += '-timer'; }, 0);</script>",
        )
        .unwrap()
        .with_initial_url("fixture://script-tasks");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine.evaluate_async("globalThis.taskOrder").await.unwrap(),
        serde_json::json!("script-micro-timer")
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_mutation_observer_delivers_script_dom_changes() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://mutation-observer",
            "<main id='root'><p id='copy'>hello</p></main>",
        )
        .unwrap()
        .with_initial_url("fixture://mutation-observer");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const root = document.getElementById('root');
                    const copy = document.getElementById('copy');
                    const log = [];
                    const observer = new MutationObserver(records => {
                        log.push(records.map(record => ({
                            type: record.type,
                            target: record.target.id || record.target.nodeName,
                            attributeName: record.attributeName,
                            oldValue: record.oldValue,
                            added: record.addedNodes.length,
                            removed: record.removedNodes.length,
                        })));
                    });
                    observer.observe(root, {
                        subtree: true,
                        attributes: true,
                        attributeOldValue: true,
                        childList: true,
                        characterData: true,
                        characterDataOldValue: true,
                    });
                    copy.setAttribute('data-state', 'one');
                    copy.setAttribute('data-state', 'two');
                    copy.firstChild.data = 'updated';
                    const extra = document.createElement('span');
                    extra.textContent = 'new';
                    root.appendChild(extra);
                    copy.remove();
                    globalThis.mutationLog = log;
                    return true;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.mutationLog")
            .await
            .unwrap(),
        serde_json::json!([[
            {
                "type": "attributes",
                "target": "copy",
                "attributeName": "data-state",
                "oldValue": null,
                "added": 0,
                "removed": 0,
            },
            {
                "type": "attributes",
                "target": "copy",
                "attributeName": "data-state",
                "oldValue": "one",
                "added": 0,
                "removed": 0,
            },
            {
                "type": "characterData",
                "target": "#text",
                "attributeName": null,
                "oldValue": "hello",
                "added": 0,
                "removed": 0,
            },
            {
                "type": "childList",
                "target": "root",
                "attributeName": null,
                "oldValue": null,
                "added": 1,
                "removed": 0,
            },
            {
                "type": "childList",
                "target": "root",
                "attributeName": null,
                "oldValue": null,
                "added": 0,
                "removed": 1,
            },
        ]])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_script_exposes_layout_geometry_and_resize_observer() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 240,
            height: 100,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://layout-observer",
            "<html><head><style>#box { width: 40px; height: 20px; } #box.grown { height: 60px; }</style></head><body><div id='spacer' style='height: 120px'></div><div id='box'>box</div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://layout-observer");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let initial = engine
        .evaluate_async(
            "(() => { const box = document.getElementById('box'); const rect = box.getBoundingClientRect(); return { rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom }, identity: [rect instanceof DOMRect, rect instanceof DOMRectReadOnly, box.getClientRects().length === 1], dimensions: [box.clientWidth, box.clientHeight, box.offsetWidth, box.offsetHeight, box.scrollWidth, box.scrollHeight] }; })()",
        )
        .await
        .unwrap();
    assert_eq!(initial["identity"], serde_json::json!([true, true, true]));
    assert_eq!(initial["rect"]["width"], serde_json::json!(40));
    assert_eq!(initial["rect"]["height"], serde_json::json!(20));
    assert_eq!(
        initial["rect"]["right"].as_i64(),
        Some(initial["rect"]["x"].as_i64().unwrap() + 40)
    );
    assert_eq!(
        initial["rect"]["bottom"].as_i64(),
        Some(initial["rect"]["y"].as_i64().unwrap() + 20)
    );
    assert_eq!(
        initial["dimensions"],
        serde_json::json!([40, 20, 40, 20, 40, 20])
    );

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const box = document.getElementById('box'); globalThis.resizeLog = []; globalThis.resizeObserver = new ResizeObserver(entries => { globalThis.resizeLog.push(entries.map(entry => [entry.target.id, entry.contentRect.width, entry.contentRect.height, entry.borderBoxSize[0].inlineSize, entry.contentBoxSize[0].blockSize])); }); globalThis.resizeObserver.observe(box, { box: 'border-box' }); return true; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("globalThis.resizeLog").await.unwrap(),
        serde_json::json!([[["box", 40, 20, 40, 20]]])
    );

    let before_scroll = engine
        .evaluate_async("document.getElementById('box').getBoundingClientRect().y")
        .await
        .unwrap();
    engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: 40,
        })
        .unwrap();
    let after_scroll = engine
        .evaluate_async("document.getElementById('box').getBoundingClientRect().y")
        .await
        .unwrap();
    assert_eq!(after_scroll, before_scroll.as_i64().unwrap() - 40);

    engine
        .evaluate_async("document.getElementById('box').setAttribute('class', 'grown'); true")
        .await
        .unwrap();
    engine
        .evaluate_async("document.getElementById('box').getBoundingClientRect().height")
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.resizeLog").await.unwrap(),
        serde_json::json!([[["box", 40, 20, 40, 20]], [["box", 40, 60, 40, 60]],])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_script_scrolls_root_and_nested_overflow_containers() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 120,
            height: 100,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://nested-scroll",
            "<html><body><div id='scroller' style='width:40px;height:30px;overflow:auto'><div id='content' style='width:80px;height:90px;background-color:blue;position:relative'><div id='marker' style='position:absolute;left:30px;top:10px;width:10px;height:10px;background-color:red'></div></div></div><div id='tail' style='height:140px'></div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://nested-scroll");
    let mut engine = NativeEngine::new(config).unwrap();
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

    let initial = engine
        .evaluate_async(
            "(() => { const scroller = document.getElementById('scroller'); const content = document.getElementById('content'); const rect = content.getBoundingClientRect(); return { dimensions: [scroller.clientWidth, scroller.clientHeight, scroller.scrollWidth, scroller.scrollHeight, scroller.scrollLeft, scroller.scrollTop], content: [rect.x, rect.y], root: [document.documentElement.scrollWidth, document.documentElement.scrollHeight, window.scrollX, window.scrollY] }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        initial["dimensions"],
        serde_json::json!([40, 30, 80, 90, 0, 0])
    );
    assert_eq!(initial["content"], serde_json::json!([8, 8]));
    assert_eq!(initial["root"][2], serde_json::json!(0));
    assert_eq!(initial["root"][3], serde_json::json!(0));
    assert!(initial["root"][1].as_i64().unwrap() > 100);
    assert_eq!(
        engine.rasterize().unwrap().pixel(39, 19),
        Some([255, 0, 0, 255]),
    );

    engine
        .evaluate_async("document.getElementById('scroller').scrollTo({ left: 20, top: 5 }); true")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.scrollEvents")
            .await
            .unwrap(),
        serde_json::json!([["scroller", true, true, false]])
    );
    assert_eq!(
        engine.rasterize().unwrap().pixel(19, 14),
        Some([255, 0, 0, 255])
    );

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); scroller.scrollTo({ left: 20, top: 25 }); window.scrollTo({ top: 40 }); return [scroller.scrollLeft, scroller.scrollTop, window.scrollX, window.scrollY, document.documentElement.scrollTop]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([20, 25, 0, 40, 40])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.scrollEvents")
            .await
            .unwrap(),
        serde_json::json!([
            ["scroller", true, true, false],
            ["scroller", true, true, false],
            ["window", true, true, false]
        ])
    );
    let moved = engine
        .evaluate_async(
            "(() => { const content = document.getElementById('content').getBoundingClientRect(); return [content.x, content.y, document.getElementById('scroller').scrollLeft, document.getElementById('scroller').scrollTop, window.scrollY]; })()",
        )
        .await
        .unwrap();
    assert_eq!(moved, serde_json::json!([-12, -57, 20, 25, 40]));

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); scroller.scrollBy(100, 100); window.scrollBy(0, 100); return [scroller.scrollLeft, scroller.scrollTop, window.scrollY]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([40, 60, 86])
    );
    let clamped = engine
        .evaluate_async(
            "(() => { const content = document.getElementById('content').getBoundingClientRect(); return [content.x, content.y, document.documentElement.scrollTop]; })()",
        )
        .await
        .unwrap();
    assert_eq!(clamped, serde_json::json!([-32, -138, 86]));

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
}
#[tokio::test]
async fn native_local_script_exposes_animation_frames_and_intersection_observer() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 240,
            height: 100,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://intersection-observer",
            "<html><head><style>#target { width: 40px; height: 20px; }</style></head><body><div style='height: 120px'></div><div id='target'>target</div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://intersection-observer");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let first = engine
        .evaluate_async(
            "(() => { const target = document.getElementById('target'); globalThis.intersectionLog = []; globalThis.frameLog = []; globalThis.intersectionObserver = new IntersectionObserver(entries => { globalThis.intersectionLog.push(entries.map(entry => ({ isIntersecting: entry.isIntersecting, ratio: entry.intersectionRatio, top: entry.boundingClientRect.top, rootHeight: entry.rootBounds.height, entryType: entry instanceof IntersectionObserverEntry }))); }, { threshold: [0, 1], rootMargin: '0px' }); globalThis.intersectionObserver.observe(target); const frameId = requestAnimationFrame(time => globalThis.frameLog.push({ time, now: performance.now() })); const cancelledId = requestAnimationFrame(() => globalThis.frameLog.push({ cancelled: true })); cancelAnimationFrame(cancelledId); return { frameId, perfNow: performance.now(), timeOrigin: performance.timeOrigin, thresholds: globalThis.intersectionObserver.thresholds, rootMargin: globalThis.intersectionObserver.rootMargin }; })()",
        )
        .await
        .unwrap();
    assert_eq!(first["frameId"], serde_json::json!(1));
    assert!(first["perfNow"].as_u64().is_some());
    assert!(first["timeOrigin"].as_i64().is_some());
    assert_eq!(first["thresholds"], serde_json::json!([0, 1]));
    assert_eq!(first["rootMargin"], serde_json::json!("0px 0px 0px 0px"));
    assert_eq!(
        engine
            .evaluate_async("globalThis.intersectionLog")
            .await
            .unwrap(),
        serde_json::json!([[
            {
                "isIntersecting": false,
                "ratio": 0,
                "top": 128,
                "rootHeight": 100,
                "entryType": true,
            }
        ]])
    );

    engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: 40,
        })
        .unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    let second = engine
        .evaluate_async("({ frames: globalThis.frameLog, intersections: globalThis.intersectionLog, perfNow: performance.now() })")
        .await
        .unwrap();
    assert_eq!(second["frames"].as_array().unwrap().len(), 1);
    assert!(second["frames"][0]["time"].as_u64().is_some());
    assert!(second["frames"][0]["now"].as_u64().is_some());
    assert_eq!(second["intersections"].as_array().unwrap().len(), 2);
    assert_eq!(
        second["intersections"][1][0]["isIntersecting"],
        serde_json::json!(true)
    );
    assert_eq!(
        second["intersections"][1][0]["ratio"],
        serde_json::json!(0.6)
    );
    assert_eq!(second["intersections"][1][0]["top"], serde_json::json!(88));
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_script_exposes_same_document_history_api() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://history-api",
            "<html><body><p>Single document application</p></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://history-api");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "({ length: history.length, state: history.state, url: location.href })"
            )
            .await
            .unwrap(),
        serde_json::json!({"length": 1, "state": null, "url": "fixture://history-api"})
    );
    engine
        .evaluate_async(
            "(() => { globalThis.popStates = []; addEventListener('popstate', event => popStates.push({ state: event.state, url: location.href })); history.pushState({ screen: 'settings', count: 1 }, '', '/settings?tab=one'); return { length: history.length, state: history.state, url: location.href }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ length: history.length, state: history.state, url: location.href })"
            )
            .await
            .unwrap(),
        serde_json::json!({"length": 2, "state": {"screen": "settings", "count": 1}, "url": "fixture://history-api/settings?tab=one"})
    );

    let replaced = engine
        .evaluate_async(
            "(() => { history.replaceState({ screen: 'profile' }, '', '?tab=two'); return { state: history.state, url: location.href }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        replaced,
        serde_json::json!({"state": {"screen": "profile"}, "url": "fixture://history-api/settings?tab=two"})
    );
    engine.evaluate_async("history.back(); true").await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ length: history.length, state: history.state, url: location.href, popStates })"
            )
            .await
            .unwrap(),
        serde_json::json!({
            "length": 2,
            "state": null,
            "url": "fixture://history-api",
            "popStates": [{"state": null, "url": "fixture://history-api"}],
        })
    );
    engine
        .evaluate_async("history.forward(); true")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({ state: history.state, url: location.href, popStates })")
            .await
            .unwrap(),
        serde_json::json!({
            "state": {"screen": "profile"},
            "url": "fixture://history-api/settings?tab=two",
            "popStates": [
                {"state": null, "url": "fixture://history-api"},
                {"state": {"screen": "profile"}, "url": "fixture://history-api/settings?tab=two"},
            ],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_script_applies_bounded_dom_commands_once() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://script-commands",
            "<input id='name' type='text'><input id='toggle' type='checkbox'><select id='choice'><option id='one' value='one'>One</option><option id='two' value='two'>Two</option></select><select id='many' multiple><option id='many-one' value='one' selected>One</option><option id='many-two' value='two'>Two</option></select>",
        )
        .unwrap()
        .with_initial_url("fixture://script-commands");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    let initial_revision = engine.revision();

    let value = engine
        .evaluate_async(
            "(() => { const input = document.getElementById('name'); input.value = 'local-value'; input.setAttribute('data-source', 'script'); return input.value; })()",
        )
        .await
        .unwrap();
    assert_eq!(value, serde_json::json!("local-value"));
    assert_eq!(engine.revision(), initial_revision + 1);
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('name').getAttribute('data-source')")
            .await
            .unwrap(),
        serde_json::json!("script")
    );
    assert_eq!(engine.revision(), initial_revision + 1);

    let checked = engine
        .evaluate_async(
            "(() => { const toggle = document.getElementById('toggle'); toggle.click(); return toggle.checked; })()",
        )
        .await
        .unwrap();
    assert_eq!(checked, serde_json::json!(true));
    assert_eq!(engine.revision(), initial_revision + 2);

    let state = engine
        .evaluate_async(
            "(() => { const toggle = document.getElementById('toggle'); const input = document.getElementById('name'); toggle.checked = false; input.removeAttribute('data-source'); return { checked: toggle.checked, source: input.getAttribute('data-source') }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        state,
        serde_json::json!({
            "checked": false,
            "source": null,
        })
    );
    assert_eq!(engine.revision(), initial_revision + 3);

    assert_eq!(
        engine
            .evaluate_async("(() => { const option = document.getElementById('two'); option.selected = true; return option.selected; })()")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(engine.revision(), initial_revision + 4);
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const select = document.getElementById('many'); select.options[1].selected = true; return { multiple: select.multiple, options: select.options.map(option => option.selected), selected: select.selectedOptions.map(option => option.value) }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "multiple": true,
            "options": [true, true],
            "selected": ["one", "two"],
        })
    );
    engine.close_async().await.unwrap();
}
#[test]
fn native_local_radio_group_arrow_navigation_revalidates_keydown_defaults() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://local-radio-arrow-navigation",
            "<form id='first'><input id='radio-a' type='radio' name='group' checked><input id='radio-b' type='radio' name='group' disabled><input id='radio-c' type='radio' name='group'></form><form id='second'><input id='radio-other-form' type='radio' name='group' checked></form><input id='radio-no-form' type='radio' name='group' checked><input id='radio-unnamed' type='radio' checked><input id='radio-solo' type='radio' name='solo' checked><input id='radio-guard' type='radio' name='guard'><input id='fieldset-radio-start' type='radio' name='fieldset-group' checked><fieldset disabled><legend><span><input id='fieldset-radio-legend' type='radio' name='fieldset-group'></span></legend><input id='fieldset-radio-disabled' type='radio' name='fieldset-group'></fieldset><input id='fieldset-radio-after' type='radio' name='fieldset-group'>",
        )
        .unwrap()
        .with_initial_url("fixture://local-radio-arrow-navigation");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "globalThis.__radioArrowTrace = []; globalThis.__radioArrowMode = ''; globalThis.__radioA = document.getElementById('radio-a'); document.addEventListener('keydown', event => { const targetId = event.target ? event.target.id : ''; __radioArrowTrace.push('keydown:' + targetId + ':' + event.key); if (__radioArrowMode === 'cancel') event.preventDefault(); if (targetId === 'radio-a') { if (__radioArrowMode === 'group') event.target.removeAttribute('name'); if (__radioArrowMode === 'focus') document.getElementById('radio-guard').focus(); if (__radioArrowMode === 'disabled') event.target.disabled = true; if (__radioArrowMode === 'type') event.target.type = 'checkbox'; if (__radioArrowMode === 'detach') event.target.remove(); } }); document.addEventListener('keyup', event => { const targetId = event.target ? event.target.id : ''; __radioArrowTrace.push('keyup:' + targetId + ':' + event.key); }); for (const type of ['input', 'change', 'click', 'pointerdown', 'pointerup', 'mousedown', 'mouseup']) document.addEventListener(type, event => { const targetId = event.target ? event.target.id : ''; __radioArrowTrace.push(type + ':' + targetId + ':' + event.bubbles); }); true",
        )
        .await
        .unwrap();

    engine
        .evaluate_async("document.getElementById('radio-a').focus(); true")
        .await
        .unwrap();
    let before = engine.revision();
    let result = engine
        .action(NativeAction::Shortcut {
            shortcut: "ArrowRight".into(),
        })
        .unwrap();
    assert_eq!(result.revision, before + 1);
    assert_eq!(
        engine
            .evaluate_async("({active: document.activeElement.id, a: document.getElementById('radio-a').checked, b: document.getElementById('radio-b').checked, c: document.getElementById('radio-c').checked, otherForm: document.getElementById('radio-other-form').checked, noForm: document.getElementById('radio-no-form').checked, trace: __radioArrowTrace})")
            .await
            .unwrap(),
        serde_json::json!({
            "active": "radio-c",
            "a": false,
            "b": false,
            "c": true,
            "otherForm": true,
            "noForm": true,
            "trace": [
                "keydown:radio-a:ArrowRight",
                "input:radio-c:true",
                "change:radio-c:true",
                "keyup:radio-c:ArrowRight",
            ],
        })
    );

    engine
        .evaluate_async("__radioArrowTrace = []; true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "ArrowDown".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({active: document.activeElement.id, a: document.getElementById('radio-a').checked, c: document.getElementById('radio-c').checked, trace: __radioArrowTrace})")
            .await
            .unwrap(),
        serde_json::json!({
            "active": "radio-a",
            "a": true,
            "c": false,
            "trace": [
                "keydown:radio-c:ArrowDown",
                "input:radio-a:true",
                "change:radio-a:true",
                "keyup:radio-a:ArrowDown",
            ],
        })
    );
    engine
        .evaluate_async("__radioArrowTrace = []; true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "ArrowLeft".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({active: document.activeElement.id, a: document.getElementById('radio-a').checked, c: document.getElementById('radio-c').checked, trace: __radioArrowTrace})")
            .await
            .unwrap(),
        serde_json::json!({
            "active": "radio-c",
            "a": false,
            "c": true,
            "trace": [
                "keydown:radio-a:ArrowLeft",
                "input:radio-c:true",
                "change:radio-c:true",
                "keyup:radio-c:ArrowLeft",
            ],
        })
    );
    engine
        .evaluate_async("__radioArrowTrace = []; true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "ArrowUp".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({active: document.activeElement.id, a: document.getElementById('radio-a').checked, c: document.getElementById('radio-c').checked, trace: __radioArrowTrace})")
            .await
            .unwrap(),
        serde_json::json!({
            "active": "radio-a",
            "a": true,
            "c": false,
            "trace": [
                "keydown:radio-c:ArrowUp",
                "input:radio-a:true",
                "change:radio-a:true",
                "keyup:radio-a:ArrowUp",
            ],
        })
    );

    engine
        .evaluate_async("__radioArrowTrace = []; document.getElementById('radio-a').focus(); true")
        .await
        .unwrap();
    let before_keydown = engine.revision();
    let keydown = engine
        .action(NativeAction::KeyDown {
            key: "ArrowRight".into(),
        })
        .unwrap();
    assert_eq!(keydown.revision, before_keydown + 1);
    assert_eq!(
        engine
            .evaluate_async("({active: document.activeElement.id, a: document.getElementById('radio-a').checked, c: document.getElementById('radio-c').checked, trace: __radioArrowTrace})")
            .await
            .unwrap(),
        serde_json::json!({
            "active": "radio-c",
            "a": false,
            "c": true,
            "trace": [
                "keydown:radio-a:ArrowRight",
                "input:radio-c:true",
                "change:radio-c:true",
            ],
        })
    );
    let keyup = engine
        .action(NativeAction::KeyUp {
            key: "ArrowRight".into(),
        })
        .unwrap();
    assert_eq!(keyup.revision, before_keydown + 2);

    for mode in ["cancel", "group", "focus", "disabled", "type", "detach"] {
        engine
            .evaluate_async(&format!(
                "(() => {{ __radioArrowTrace = []; __radioArrowMode = '{mode}'; const a = document.getElementById('radio-a'); if (a) {{ a.disabled = false; a.type = 'radio'; a.setAttribute('name', 'group'); a.checked = true; a.focus(); }} return true; }})()"
            ))
            .await
            .unwrap();
        engine
            .action(NativeAction::Shortcut {
                shortcut: "ArrowRight".into(),
            })
            .unwrap();
        let state = engine
            .evaluate_async("({active: __radioArrowMode === 'focus' && document.activeElement ? document.activeElement.id : null, a: document.getElementById('radio-a') ? document.getElementById('radio-a').checked : (globalThis.__radioA ? globalThis.__radioA.checked : null), c: document.getElementById('radio-c').checked, trace: __radioArrowTrace})")
            .await
            .unwrap();
        assert_eq!(state["c"], false, "radio moved during {mode} invalidation");
        assert!(
            !state["trace"].as_array().unwrap().iter().any(|event| {
                event.as_str().is_some_and(|event| {
                    event.starts_with("input:") || event.starts_with("change:")
                })
            }),
            "selection events fired during {mode} invalidation: {state}"
        );
        if mode == "focus" {
            assert_eq!(state["active"], "radio-guard");
        }
        if mode != "detach" {
            assert_eq!(
                state["a"], true,
                "source radio changed during {mode} invalidation"
            );
        }
    }

    engine
        .evaluate_async("__radioArrowTrace = []; __radioArrowMode = ''; document.getElementById('radio-solo').focus(); true")
        .await
        .unwrap();
    let before_unchanged_shortcut = engine.revision();
    let unchanged = engine
        .action(NativeAction::Shortcut {
            shortcut: "ArrowDown".into(),
        })
        .unwrap();
    assert_eq!(unchanged.revision, before_unchanged_shortcut + 1);
    assert_eq!(
        engine
            .evaluate_async("({active: document.activeElement.id, checked: document.getElementById('radio-solo').checked, trace: __radioArrowTrace})")
            .await
            .unwrap(),
        serde_json::json!({
            "active": "radio-solo",
            "checked": true,
            "trace": ["keydown:radio-solo:ArrowDown", "keyup:radio-solo:ArrowDown"],
        })
    );

    engine
        .evaluate_async(
            "__radioArrowTrace = []; document.getElementById('radio-unnamed').focus(); true",
        )
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "ArrowRight".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({active: document.activeElement.id, checked: document.getElementById('radio-unnamed').checked, trace: __radioArrowTrace})")
            .await
            .unwrap(),
        serde_json::json!({
            "active": "radio-unnamed",
            "checked": true,
            "trace": ["keydown:radio-unnamed:ArrowRight", "keyup:radio-unnamed:ArrowRight"],
        })
    );

    engine
        .evaluate_async(
            "__radioArrowTrace = []; document.getElementById('fieldset-radio-start').focus(); true",
        )
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "ArrowRight".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({active: document.activeElement.id, legend: document.getElementById('fieldset-radio-legend').checked, disabled: document.getElementById('fieldset-radio-disabled').checked, after: document.getElementById('fieldset-radio-after').checked, trace: __radioArrowTrace})")
            .await
            .unwrap(),
        serde_json::json!({
            "active": "fieldset-radio-legend",
            "legend": true,
            "disabled": false,
            "after": false,
            "trace": [
                "keydown:fieldset-radio-start:ArrowRight",
                "input:fieldset-radio-legend:true",
                "change:fieldset-radio-legend:true",
                "keyup:fieldset-radio-legend:ArrowRight",
            ],
        })
    );
    engine
        .evaluate_async("__radioArrowTrace = []; true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "ArrowDown".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({active: document.activeElement.id, legend: document.getElementById('fieldset-radio-legend').checked, disabled: document.getElementById('fieldset-radio-disabled').checked, after: document.getElementById('fieldset-radio-after').checked, trace: __radioArrowTrace})")
            .await
            .unwrap(),
        serde_json::json!({
            "active": "fieldset-radio-after",
            "legend": false,
            "disabled": false,
            "after": true,
            "trace": [
                "keydown:fieldset-radio-legend:ArrowDown",
                "input:fieldset-radio-after:true",
                "change:fieldset-radio-after:true",
                "keyup:fieldset-radio-after:ArrowDown",
            ],
        })
    );
    engine.close_async().await.unwrap();
        });
    });
}
#[tokio::test]
async fn native_local_semantic_submit_button_navigates_without_script_realm() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://semantic-form",
            "<form action='fixture://semantic-result'><input name='query' value='hello'><button id='go' type='submit' style='width:80px;height:20px'>Go</button></form>",
        )
        .unwrap()
        .with_fixture(
            "fixture://semantic-result?query=hello",
            "<title>Result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://semantic-form");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let result = engine
        .action(NativeAction::Click {
            target: "id=go".into(),
        })
        .unwrap();
    assert!(result.accepted);
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://semantic-result?query=hello"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Result");
}
#[tokio::test]
async fn native_local_inline_script_failure_dispatches_error_without_aborting_document() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://inline-script-error",
            r#"
                <script>
                    globalThis.inlineEvents = [];
                    globalThis.windowEvents = [];
                    globalThis.onerrorEvents = [];
                    globalThis.windowCancelledEvents = [];
                    window.addEventListener('error', event => windowEvents.push([
                        event.message,
                        event.filename,
                        event.lineno,
                        event.colno,
                        event.error instanceof Error,
                        event.target === window,
                        event.cancelable,
                    ]));
                    window.onerror = (message, filename, line, column, error) => {
                        onerrorEvents.push([
                            message,
                            filename,
                            line,
                            column,
                            error instanceof Error,
                        ]);
                        return true;
                    };
                    window.addEventListener('error', event => windowCancelledEvents.push(event.defaultPrevented));
                    const brokenClassic = document.getElementById('broken-classic');
                    brokenClassic.addEventListener('error', event => inlineEvents.push([
                        'classic',
                        event.message,
                        event.filename,
                        event.error instanceof Error,
                    ]));
                    const brokenModule = document.getElementById('broken-module');
                    brokenModule.addEventListener('error', event => inlineEvents.push([
                        'module',
                        event.message,
                        event.filename,
                        event.error instanceof Error,
                    ]));
                </script>
                <script id='broken-classic'>
                    throw new Error('inline classic boom');
                    globalThis.inlineRan = true;
                </script>
                <script id='broken-module' type='module'>
                    throw new Error('inline module boom');
                    globalThis.moduleInlineRan = true;
                </script>
                <title>Inline error is isolated</title>
                <p>Document committed</p>
            "#,
        )
        .unwrap()
        .with_initial_url("fixture://inline-script-error");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.url, "fixture://inline-script-error");
    assert_eq!(snapshot.title, "Inline error is isolated");
    assert_eq!(snapshot.visible_text, "Document committed");
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.inlineEvents.map(event => [event[0], event[1].includes(event[0] === 'classic' ? 'inline classic boom' : 'inline module boom'), event[2], event[3]])",
            )
            .await
            .unwrap(),
        serde_json::json!([
            ["classic", true, "fixture://inline-script-error", true],
            ["module", true, "fixture://inline-script-error", true],
        ])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.windowEvents.map(event => [event[0].includes('inline classic boom'), event[1], event[2], event[3], event[4], event[5], event[6]])",
            )
            .await
        .unwrap(),
        serde_json::json!([
            [true, "fixture://inline-script-error", 0, 0, true, true, true],
            [false, "fixture://inline-script-error", 0, 0, true, true, true],
        ])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.onerrorEvents.map(event => [event[0].includes('inline classic boom'), event[1], event[2], event[3], event[4]])",
            )
            .await
        .unwrap(),
        serde_json::json!([
            [true, "fixture://inline-script-error", 0, 0, true],
            [false, "fixture://inline-script-error", 0, 0, true],
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.windowCancelledEvents")
            .await
            .unwrap(),
        serde_json::json!([true, true])
    );
    assert_eq!(
        engine
            .evaluate_async(native_window_onerror_return_semantics_script())
            .await
            .unwrap(),
        serde_json::json!([
            [
                false,
                true,
                false,
                [true, "reported value", "native.js", 3, 7, true]
            ],
            [true, false],
            [true, false, [true, "error", true]],
            [true, false, true, true],
        ])
    );
    assert_eq!(
        engine
            .evaluate_async(native_event_handler_false_cancellation_script())
            .await
            .unwrap(),
        serde_json::json!([
            [false, false, true, [["handler", true], ["later", true]],],
            [false, false, true, [true], [["later", true]]],
            [true, false],
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("[Boolean(globalThis.inlineRan), Boolean(globalThis.moduleInlineRan)]")
            .await
            .unwrap(),
        serde_json::json!([false, false])
    );
    assert_eq!(
        engine.evaluate_async("document.readyState").await.unwrap(),
        serde_json::json!("complete")
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dispatches_large_script_error_batch_as_data() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://large-script-error-batch",
            r#"
                <script>
                    globalThis.errorLengths = [];
                    window.addEventListener('error', event => errorLengths.push(event.message.length));
                </script>
                <script>throw new Error('x'.repeat(4096));</script>
                <script>throw new Error('x'.repeat(4096));</script>
                <script>throw new Error('x'.repeat(4096));</script>
                <script>throw new Error('x'.repeat(4096));</script>
                <script>throw new Error('x'.repeat(4096));</script>
                <main>Large script error batch</main>
            "#,
        )
        .unwrap()
        .with_initial_url("fixture://large-script-error-batch");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("[errorLengths.length, errorLengths.every(length => length >= 4000)]")
            .await
            .unwrap(),
        serde_json::json!([5, true])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_canvas_2d_script_and_raster_pipeline() {
    let image_url = native_test_png_data_url();
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 32,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://canvas",
            format!("<html><body><canvas id='canvas' width='8' height='6' style='display:block;width:8px;height:6px'></canvas><img id='source' src='{image_url}' style='display:none'></body></html>"),
        )
        .unwrap()
        .with_initial_url("fixture://canvas");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let initial = engine
        .evaluate_async(
            "(() => { const canvas = document.getElementById('canvas'); const context = canvas.getContext('2d'); globalThis.nativeCanvas = canvas; globalThis.nativeCanvasContext = context; const rect = canvas.getBoundingClientRect(); return { dimensions: [canvas.width, canvas.height, rect.x, rect.y, rect.width, rect.height], identity: [canvas instanceof HTMLCanvasElement, context instanceof CanvasRenderingContext2D, canvas.getContext('webgl') === null, context.getContextAttributes().alpha] }; })()",
        )
        .await
        .unwrap();
    assert_eq!(initial["dimensions"], serde_json::json!([8, 6, 8, 8, 8, 6]));
    assert_eq!(
        initial["identity"],
        serde_json::json!([true, true, true, true])
    );

    let drawn = engine
        .evaluate_async(
            "(() => { const canvas = nativeCanvas; const context = nativeCanvasContext; const source = document.getElementById('source'); context.fillStyle = '#ff0000'; context.fillRect(1, 1, 3, 2); context.clearRect(2, 1, 1, 1); const copy = context.createImageData(1, 1); copy.data.set([0, 255, 0, 255]); context.putImageData(copy, 5, 2); context.drawImage(source, 0, 4); const image = context.getImageData(0, 0, 4, 3); const sourcePixel = Array.from(context.getImageData(0, 4, 1, 1).data); const pixel = (value, x, y) => Array.from(value.data.slice((y * value.width + x) * 4, (y * value.width + x + 1) * 4)); const output = { samples: [pixel(image, 1, 1), pixel(image, 2, 1), pixel(image, 3, 1)], copy: pixel(context.getImageData(5, 2, 1, 1), 0, 0), source: sourcePixel, png: canvas.toDataURL().startsWith('data:image/png;base64,iVBORw0KGgo'), imageData: [image instanceof ImageData, image.data instanceof Uint8ClampedArray] }; return output; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        drawn["samples"],
        serde_json::json!([[255, 0, 0, 255], [0, 0, 0, 0], [255, 0, 0, 255]])
    );
    assert_eq!(drawn["copy"], serde_json::json!([0, 255, 0, 255]));
    assert_eq!(drawn["source"], serde_json::json!([255, 0, 0, 255]));
    assert_eq!(drawn["imageData"], serde_json::json!([true, true]));
    assert_eq!(drawn["png"], serde_json::json!(true));

    let bitmap = engine
        .evaluate_async(
            "await (async () => { const source = document.getElementById('source'); const bitmap = await createImageBitmap(source, 0, 0, 1, 1, { resizeWidth: 2, resizeHeight: 2 }); const context = nativeCanvas.getContext('2d'); const nativeGreenBefore = Array.from(context.getImageData(5, 2, 1, 1).data); context.drawImage(bitmap, 6, 4); const pixel = Array.from(context.getImageData(6, 4, 1, 1).data); const identity = [bitmap instanceof ImageBitmap, bitmap.width, bitmap.height]; bitmap.close(); let closed = ''; try { context.drawImage(bitmap, 0, 0); } catch (error) { closed = error.name; } return { identity, pixel, nativeGreenBefore, closed: [closed, bitmap.width, bitmap.height] }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        bitmap,
        serde_json::json!({
            "identity": [true, 2, 2],
            "pixel": [255, 0, 0, 255],
            "nativeGreenBefore": [0, 255, 0, 255],
            "closed": ["InvalidStateError", 0, 0]
        })
    );

    let offscreen = engine
        .evaluate_async(
            "await (async () => { const placeholder = document.createElement('canvas'); placeholder.width = 3; placeholder.height = 2; document.body.appendChild(placeholder); const offscreen = placeholder.transferControlToOffscreen(); const context = offscreen.getContext('2d'); context.fillStyle = '#0000ff'; context.fillRect(0, 0, 3, 2); const frame = offscreen.transferToImageBitmap(); nativeCanvasContext.drawImage(frame, 4, 0); const pixel = Array.from(nativeCanvasContext.getImageData(4, 0, 1, 1).data); const nativeGreen = Array.from(nativeCanvasContext.getImageData(5, 2, 1, 1).data); let placeholderError = ''; try { placeholder.getContext('2d'); } catch (error) { placeholderError = error.name; } const blob = await offscreen.convertToBlob(); const standalone = new OffscreenCanvas(2, 1); const standaloneContext = standalone.getContext('2d'); standaloneContext.fillStyle = '#00ff00'; standaloneContext.fillRect(0, 0, 2, 1); const standaloneFrame = standalone.transferToImageBitmap(); return { identity: [offscreen instanceof OffscreenCanvas, frame instanceof ImageBitmap, standalone instanceof OffscreenCanvas, standaloneFrame instanceof ImageBitmap], dimensions: [offscreen.width, offscreen.height, frame.width, frame.height, standalone.width, standalone.height], pixel, nativeGreen, placeholderError, blob: [blob instanceof Blob, blob.type, blob.size > 0], standalonePixel: Array.from(standaloneContext.getImageData(1, 0, 1, 1).data) }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        offscreen,
        serde_json::json!({
            "identity": [true, true, true, true],
            "dimensions": [3, 2, 3, 2, 2, 1],
            "pixel": [0, 0, 255, 255],
            "nativeGreen": [0, 255, 0, 255],
            "placeholderError": "InvalidStateError",
            "blob": [true, "image/png", true],
            "standalonePixel": [0, 255, 0, 255]
        })
    );

    let advanced = engine
        .evaluate_async(
            "(() => { const canvas = document.createElement('canvas'); canvas.width = 4; canvas.height = 4; document.body.appendChild(canvas); const context = canvas.getContext('2d'); context.strokeStyle = '#00ff00'; context.strokeRect(0.5, 0.5, 3, 3); const border = Array.from(context.getImageData(0, 0, 1, 1).data); context.save(); context.globalAlpha = 0.25; context.lineWidth = 4; context.restore(); const gradient = context.createLinearGradient(0, 0, 4, 0); gradient.addColorStop(0, '#ff0000'); gradient.addColorStop(1, '#0000ff'); context.fillStyle = gradient; context.fillRect(0, 1, 4, 1); const left = Array.from(context.getImageData(0, 1, 1, 1).data); const right = Array.from(context.getImageData(3, 1, 1, 1).data); canvas.width = canvas.width; const reset = Array.from(canvas.getContext('2d').getImageData(0, 0, 1, 1).data); const state = [context.globalAlpha, context.lineWidth, gradient instanceof CanvasGradient]; const nativeGreen = Array.from(nativeCanvasContext.getImageData(5, 2, 1, 1).data); canvas.remove(); return { border, left, right, reset, state, nativeGreen }; })()",
        )
        .await
        .unwrap();
    assert_eq!(advanced["border"], serde_json::json!([0, 255, 0, 255]));
    assert!(advanced["left"][0].as_u64().unwrap() > advanced["left"][2].as_u64().unwrap());
    assert!(advanced["right"][2].as_u64().unwrap() > advanced["right"][0].as_u64().unwrap());
    assert_eq!(advanced["reset"], serde_json::json!([0, 0, 0, 0]));
    assert_eq!(advanced["state"], serde_json::json!([1, 1, true]));
    assert_eq!(advanced["nativeGreen"], serde_json::json!([0, 255, 0, 255]));

    let paths = engine
        .evaluate_async(
            "(() => { const canvas = document.createElement('canvas'); canvas.width = 8; canvas.height = 4; document.body.appendChild(canvas); const context = canvas.getContext('2d'); const path = new Path2D(); path.rect(1, 1, 2, 2); const copied = new Path2D(path); const parsed = new Path2D('M 4 1 h 2 v 2 h -2 z'); copied.addPath(parsed, { a: 1, b: 0, c: 0, d: 1, e: 1, f: 0 }); context.fillStyle = '#ff0000'; context.fill(copied); const pixel = (x, y) => Array.from(context.getImageData(x, y, 1, 1).data); const filled = [pixel(0, 0), pixel(1, 1), pixel(6, 2)]; const pathQueries = [path instanceof Path2D, copied instanceof Path2D, context.isPointInPath(copied, 1.5, 1.5), context.isPointInPath(copied, 0, 0)]; context.beginPath(); context.rect(2, 0, 2, 4); const currentQuery = context.isPointInPath(2.5, 1.5); context.save(); context.clip(); context.fillStyle = '#0000ff'; context.fillRect(0, 0, 8, 4); const clipped = [pixel(1, 0), pixel(2, 0), pixel(4, 0)]; context.restore(); context.fillStyle = '#00ff00'; context.fillRect(0, 0, 1, 1); return { filled, pathQueries, currentQuery, clipped, restored: pixel(0, 0) }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        paths,
        serde_json::json!({
            "filled": [[0, 0, 0, 0], [255, 0, 0, 255], [255, 0, 0, 255]],
            "pathQueries": [true, true, true, false],
            "currentQuery": true,
            "clipped": [[0, 0, 0, 0], [0, 0, 255, 255], [0, 0, 0, 0]],
            "restored": [0, 255, 0, 255]
        })
    );

    let matrices = engine
        .evaluate_async(
            "(() => { const matrix = new DOMMatrix([1, 0, 0, 1, 2, 1]); const product = matrix.multiply(new DOMMatrix().scale(2)); const point = new DOMPoint(1, 2).matrixTransform(matrix); const translated = matrix.translate(1, 1); const canvas = document.createElement('canvas'); canvas.width = 4; canvas.height = 3; document.body.appendChild(canvas); const context = canvas.getContext('2d'); context.setTransform(matrix); context.fillStyle = '#ff0000'; context.fillRect(0, 0, 1, 1); const transform = context.getTransform(); return { identity: [matrix instanceof DOMMatrix, matrix instanceof DOMMatrixReadOnly, point instanceof DOMPoint, point instanceof DOMPointReadOnly, transform instanceof DOMMatrixReadOnly], values: [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f], product: [product.a, product.d, product.e, product.f], translated: [translated.e, translated.f], point: [point.x, point.y], arrays: [matrix.toFloat32Array().length, matrix.toFloat64Array().length], pixel: Array.from(context.getImageData(2, 1, 1, 1).data) }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        matrices,
        serde_json::json!({
            "identity": [true, true, true, true, true],
            "values": [1, 0, 0, 1, 2, 1],
            "product": [2, 2, 2, 1],
            "translated": [3, 2],
            "point": [3, 3],
            "arrays": [6, 6],
            "pixel": [255, 0, 0, 255]
        })
    );

    let persisted = engine
        .evaluate_async(
            "(() => { const canvas = nativeCanvas; const context = canvas.getContext('2d'); const sample = context.getImageData(5, 2, 1, 1); return [canvas.getContext('2d') === nativeCanvasContext, Array.from(sample.data), canvas.width, canvas.height]; })()",
        )
        .await
        .unwrap();
    assert_eq!(persisted, serde_json::json!([true, [0, 255, 0, 255], 8, 6]));

    let surface = engine.rasterize().unwrap();
    assert_eq!(surface.pixel(9, 9), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(10, 9), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(11, 9), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(13, 10), Some([0, 255, 0, 255]));
    engine.close_async().await.unwrap();
}
#[test]
fn native_local_presentation_important_priority_reaches_hidden_and_opacity_owners() {
    let document = NativeDocument::parse(
        r#"<style>
        .box { display:block; width:24px; height:8px; background-color:red; }
        @layer base {
          #display-important { display:none !important; visibility:visible !important; opacity:100% !important; }
          #visibility-important { display:block !important; visibility:hidden !important; opacity:100% !important; }
          #opacity-important { display:block !important; visibility:visible !important; opacity:25% !important; }
          #rollback { display:revert-layer !important; visibility:revert-layer !important; opacity:revert-layer !important; }
          #invalid { display:none !important; visibility:hidden !important; opacity:25% !important; }
          #normal { display:none; visibility:hidden; opacity:25%; }
        }
        @layer theme {
          #display-important { display:block !important; visibility:visible !important; opacity:75% !important; }
          #visibility-important { display:block !important; visibility:visible !important; opacity:75% !important; }
          #opacity-important { display:block !important; visibility:visible !important; opacity:75% !important; }
          #rollback { display:block !important; visibility:visible !important; opacity:75% !important; }
          #invalid { display:unsupported !important; visibility:unsupported !important; opacity:2 !important; }
          #normal { display:block; visibility:visible; opacity:75%; }
        }
        @layer top {
          #rollback { display:none !important; visibility:hidden !important; opacity:25% !important; }
        }
        #display-important { display:block; visibility:visible; opacity:100%; }
        #visibility-important { display:block; visibility:visible; opacity:100%; }
        #opacity-important { display:block; visibility:visible; opacity:100%; }
        #rollback { display:block; visibility:visible; opacity:100%; }
        #invalid { display:block; visibility:visible; opacity:100%; }
        #normal { display:block; visibility:visible; opacity:100%; }
        #inline { display:block !important; visibility:visible !important; opacity:75% !important; }
        #inline-rollback { display:none !important; visibility:hidden !important; opacity:25% !important; }
        #inline-rollback { display:block; visibility:visible; opacity:100%; }
        </style>
        <button id='display-important' class='box'>Display</button>
        <button id='visibility-important' class='box'>Visibility</button>
        <button id='opacity-important' class='box'>Opacity</button>
        <button id='rollback' class='box'>Rollback</button>
        <button id='invalid' class='box'>Invalid</button>
        <button id='normal' class='box'>Normal</button>
        <button id='inline' class='box' style='display:none !important;visibility:hidden !important;opacity:50% !important'>Inline</button>
        <button id='inline-rollback' class='box' style='display:revert-layer !important;visibility:revert-layer !important;opacity:revert-layer !important'>Inline rollback</button>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let display_important = document.resolve_target("id=display-important").unwrap();
    let visibility_important = document.resolve_target("id=visibility-important").unwrap();
    let opacity_important = document.resolve_target("id=opacity-important").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let inline_rollback = document.resolve_target("id=inline-rollback").unwrap();

    let layout = document.layout(viewport).unwrap();
    for node_id in [display_important, visibility_important, invalid, inline] {
        assert_eq!(layout.box_for(node_id), None);
    }
    for node_id in [opacity_important, rollback, normal, inline_rollback] {
        assert!(layout.box_for(node_id).is_some());
    }
    assert_eq!(layout.hit_test(9, 1).unwrap(), Some(opacity_important));
    assert_eq!(
        layout.hit_test(9, 9).unwrap(),
        Some(rollback),
        "important rollback should preserve the theme value"
    );
    assert_eq!(layout.hit_test(9, 17).unwrap(), Some(normal));
    assert_eq!(layout.hit_test(9, 25).unwrap(), Some(inline_rollback));

    let semantic = document.semantic_nodes();
    for node_id in [display_important, visibility_important, invalid, inline] {
        assert!(
            semantic
                .iter()
                .any(|node| node.node_id == node_id && node.hidden)
        );
    }
    for node_id in [opacity_important, rollback, normal, inline_rollback] {
        assert!(
            semantic
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden)
        );
    }

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
    assert_eq!(
        groups,
        vec![(opacity_important, 64), (rollback, 191)],
        "only the resolved opacity values should create groups"
    );
    for node_id in [display_important, visibility_important, invalid, inline] {
        assert!(!list.commands.iter().any(|command| match command {
            NativeDisplayCommand::FillRect {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::BorderRect {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::TextRun {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::GlyphRun {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::SvgStroke {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::SvgPolygonFill {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::SvgPolyline {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::SvgPathFill {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::SvgPathStroke {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::Image {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::SvgImage {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::BeginOpacityGroup {
                node_id: command_node,
                ..
            }
            | NativeDisplayCommand::EndOpacityGroup {
                node_id: command_node,
            } => {
                *command_node == node_id
            }
            NativeDisplayCommand::Clear { .. }
            | NativeDisplayCommand::SetNestedScrollOffset { .. } => false,
        }));
    }
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, .. } if *node_id == opacity_important
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, .. } if *node_id == normal
        )
    }));
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "display" | "visibility" | "opacity"
            )
    }));

    let surface = list.rasterize().unwrap();
    let background_pixel = |node_id, color| {
        let rectangle = layout.box_for(node_id).unwrap();
        assert_eq!(
            surface.pixel(
                rectangle.right().saturating_sub(1),
                rectangle.y.saturating_add(rectangle.height / 2),
            ),
            Some(color)
        );
    };
    background_pixel(opacity_important, [255, 191, 191, 255]);
    background_pixel(rollback, [255, 64, 64, 255]);
    background_pixel(normal, [255, 0, 0, 255]);
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (32, 96));
}

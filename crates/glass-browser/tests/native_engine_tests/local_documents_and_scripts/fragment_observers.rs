use super::*;

#[tokio::test]
async fn native_local_template_inner_html_observes_replaced_fragment_children() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://template-inner-html-observer",
            "<template id='content'><b>old child</b><!--old comment-->old tail</template>",
        )
        .unwrap()
        .with_initial_url("fixture://template-inner-html-observer");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const template = document.getElementById('content'); const content = template.content; globalThis.templateContent = content; globalThis.oldTemplateChildren = Array.from(content.childNodes); globalThis.templateMutationRecords = []; const observer = new MutationObserver(records => templateMutationRecords.push(...records)); observer.observe(content, { childList: true }); template.innerHTML = 'new text<!--new comment--><section><i>nested</i></section>tail'; globalThis.newTemplateChildren = Array.from(content.childNodes); return [content.childNodes.length, content.innerHTML]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([4, "new text<!--new comment--><section><i>nested</i></section>tail"])
    );

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const content = globalThis.templateContent; const records = globalThis.templateMutationRecords; const record = records[0]; const oldNodes = globalThis.oldTemplateChildren; const newNodes = globalThis.newTemplateChildren; return { recordCount: records.length, target: record && record.target === content, removed: record && Array.from(record.removedNodes).map(node => node.nodeName), added: record && Array.from(record.addedNodes).map(node => node.nodeName), removedIdentity: record && Array.from(record.removedNodes).every((node, index) => node === oldNodes[index]), addedIdentity: record && Array.from(record.addedNodes).every((node, index) => node === newNodes[index]), oldDetached: oldNodes.every(node => node.parentNode === null), content: [content.parentNode === null, content.childNodes.length, content.firstChild === newNodes[0], content.lastChild === newNodes[3], newNodes[0].parentNode === content, newNodes[1].parentNode === content, newNodes[2].parentNode === content, newNodes[2].firstChild.parentNode === newNodes[2], newNodes[0].nextSibling === newNodes[1], newNodes[1].nextSibling === newNodes[2], newNodes[2].nextSibling === newNodes[3], newNodes[3].previousSibling === newNodes[2], content.innerHTML, newNodes[2].innerHTML, newNodes[2].firstChild.textContent], nested: [newNodes[2].firstChild.innerHTML, newNodes[2].firstChild.textContent, newNodes[2].firstChild.childNodes.length, Array.from(newNodes[2].firstChild.childNodes).map(node => [node.nodeName, node.textContent, node.__glassMarkup])] }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "recordCount": 1,
            "target": true,
            "removed": ["B", "#comment", "#text"],
            "added": ["#text", "#comment", "SECTION", "#text"],
            "removedIdentity": true,
            "addedIdentity": true,
            "oldDetached": true,
            "content": [
                true,
                4,
                true,
                true,
                true,
                true,
                true,
                true,
                true,
                true,
                true,
                true,
                "new text<!--new comment--><section><i>nested</i></section>tail",
                "<i>nested</i>",
                "nested",
            ],
            "nested": ["nested", "nested", 1, [["#text", "nested", "nested"]]],
        })
    );
    engine.close_async().await.unwrap();
}

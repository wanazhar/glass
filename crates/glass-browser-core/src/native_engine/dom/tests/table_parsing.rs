use super::*;

#[test]
fn javascript_inner_html_table_foster_preview_matches_committed_tree() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("table-fragment-foster-test")
        .expect("native JavaScript runtime must construct");
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default()).unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const root = document.querySelector("#root");
                    root.innerHTML = "<table><tbody>before<div id='foster'><span id='nested'>inside</span></div> &#32;<!--kept--><tr><td>cell</td></tr></tbody></table>";
                    const foster = root.querySelector("#foster");
                    const nested = root.querySelector("#nested");
                    const table = root.querySelector("table");
                    return [
                        root.firstElementChild === foster,
                        foster.parentElement === root,
                        foster.nextElementSibling === table,
                        nested.parentElement === foster,
                    ];
                })()"##,
                &document,
                "fixture://table-fragment-foster.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("table foster insertion preview must evaluate");

    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("the fragment projection must commit through the native parser");

    let root = document.find_element_by_id("root").unwrap();
    let foster = document.find_element_by_id("foster").unwrap();
    let nested = document.find_element_by_id("nested").unwrap();
    assert_eq!(
        document.node(foster).and_then(NativeNode::parent),
        Some(root)
    );
    assert_eq!(
        document.node(nested).and_then(NativeNode::parent),
        Some(foster)
    );
    assert_eq!(
        document.element_inner_html(root, 1024),
        "before<div id=\"foster\"><span id=\"nested\">inside</span></div><table><tbody>  <!--kept--><tr><td>cell</td></tr></tbody></table>"
    );
}

#[test]
fn javascript_inner_html_projects_implied_table_containers_before_commit() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("implied-table-fragment-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
        "<main id='root'></main><table id='target'></table>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const root = document.querySelector("#root");
                    root.innerHTML = "<table id='rows'><tr id='row'><td id='cell'>row</td></tr></table><table id='section'><tbody id='body'><th id='section-cell'>section</th></tbody></table>";
                    const target = document.querySelector("#target");
                    target.innerHTML = "<td id='direct-cell'>direct</td>";
                    const table = root.querySelector("#rows");
                    const body = table.firstElementChild;
                    const row = body.firstElementChild;
                    const section = root.querySelector("#body");
                    return [
                        body.localName === "tbody",
                        row.localName === "tr" && row.parentElement === body,
                        row.firstElementChild.parentElement === row,
                        section.firstElementChild.localName === "tr",
                        target.firstElementChild.localName === "tbody",
                        target.firstElementChild.firstElementChild.localName === "tr",
                    ];
                })()"##,
                &document,
                "fixture://implied-table-fragment.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("implicit table containers must project in the same turn");

    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true, true])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("the projected fragment must commit through the native parser");

    let root = document.find_element_by_id("root").unwrap();
    let table = document.find_element_by_id("rows").unwrap();
    let body = document.node(table).unwrap().children()[0];
    let row = document.node(body).unwrap().children()[0];
    let cell = document.find_element_by_id("cell").unwrap();
    assert_eq!(document.node(cell).unwrap().parent(), Some(row));
    assert_eq!(
        document.element_inner_html(root, 2048),
        "<table id=\"rows\"><tbody><tr id=\"row\"><td id=\"cell\">row</td></tr></tbody></table><table id=\"section\"><tbody id=\"body\"><tr><th id=\"section-cell\">section</th></tr></tbody></table>"
    );

    let target = document.find_element_by_id("target").unwrap();
    assert_eq!(
        document.element_inner_html(target, 1024),
        "<tbody><tr><td id=\"direct-cell\">direct</td></tr></tbody>"
    );
}

#[test]
fn javascript_inner_html_projects_implied_colgroup_before_commit() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("implied-colgroup-fragment-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
        "<main id='root'></main><table id='target'></table>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const root = document.querySelector("#root");
                    root.innerHTML = "<table id='columns'><col id='first'><col id='second'></table><table id='explicit'><colgroup id='group'><col id='third'></colgroup></table>";
                    const target = document.querySelector("#target");
                    target.innerHTML = "<col id='target-column'>";
                    const columns = root.querySelector("#columns");
                    const group = columns.firstElementChild;
                    const explicit = root.querySelector("#explicit");
                    return [
                        group.localName === "colgroup",
                        group.children.length === 2,
                        explicit.children.length === 1
                            && explicit.firstElementChild === root.querySelector("#group"),
                        target.firstElementChild.localName === "colgroup",
                        target.firstElementChild.firstElementChild === target.querySelector("#target-column"),
                    ];
                })()"##,
                &document,
                "fixture://implied-colgroup-fragment.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("implied column group must project in the same turn");

    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("the projected colgroup tree must commit through the native parser");
    let root = document.find_element_by_id("root").unwrap();
    assert_eq!(
        document.element_inner_html(root, 2048),
        "<table id=\"columns\"><colgroup><col id=\"first\"><col id=\"second\"></colgroup></table><table id=\"explicit\"><colgroup id=\"group\"><col id=\"third\"></colgroup></table>"
    );
    let target = document.find_element_by_id("target").unwrap();
    assert_eq!(
        document.element_inner_html(target, 1024),
        "<colgroup><col id=\"target-column\"></colgroup>"
    );
}

#[test]
fn javascript_inner_html_recovers_nested_table_starts_by_fragment_context() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("nested-table-fragment-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
        "<table id='target'></table><table id='nested-target'></table>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const target = document.querySelector("#target");
                    target.innerHTML = "<table id='ignored'><tr id='flattened-row'><td id='flattened-cell'>row</td></tr></table>";
                    const nestedTarget = document.querySelector("#nested-target");
                    nestedTarget.innerHTML = "<tbody><tr><td id='nested-cell'><table id='valid-nested'><tr><td>nested</td></tr></table></td></tr></tbody>";
                    const flattenedBody = target.firstElementChild;
                    const flattenedRow = flattenedBody.firstElementChild;
                    const nestedCell = nestedTarget.querySelector("#nested-cell");
                    const validNested = nestedTarget.querySelector("#valid-nested");
                    return [
                        target.querySelector("#ignored") === null,
                        flattenedBody.localName === "tbody",
                        flattenedRow.localName === "tr" && flattenedRow.firstElementChild.id === "flattened-cell",
                        nestedCell.firstElementChild === validNested,
                        validNested.parentElement === nestedCell,
                    ];
                })()"##,
                &document,
                "fixture://nested-table-fragment.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("fragment table starts must respect context insertion mode");

    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("fragment table recovery must commit through the native parser");

    let target = document.find_element_by_id("target").unwrap();
    assert_eq!(
        document.element_inner_html(target, 1024),
        "<tbody><tr id=\"flattened-row\"><td id=\"flattened-cell\">row</td></tr></tbody>"
    );
    let nested_target = document.find_element_by_id("nested-target").unwrap();
    assert_eq!(
        document.element_inner_html(nested_target, 2048),
        "<tbody><tr><td id=\"nested-cell\"><table id=\"valid-nested\"><tbody><tr><td>nested</td></tr></tbody></table></td></tr></tbody>"
    );
}

#[test]
fn xhr_html_response_document_fosters_the_shared_table_fixture() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("xhr-html-foster-test")
        .expect("native JavaScript runtime must construct");
    let document =
        NativeDocument::parse("<main id='host'></main>", &NativeEngineLimits::default()).unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const response = globalThis.__glassParseHtmlDocument(
                        "<main id='root'><table><tbody id='section'>before<div id='foster'><span id='nested'>inside</span></div> &#32;<!--kept--><tr><td>cell</td></tr></tbody></table></main>",
                        "https://example.test/response.html",
                        "text/html",
                    );
                    if (!response) return null;
                    const root = response.getElementById("root");
                    const foster = response.getElementById("foster");
                    const nested = response.getElementById("nested");
                    const table = response.getElementsByTagName("table")[0];
                    const section = response.getElementById("section");
                    const rootChildren = Array.from(root.children);
                    const sectionChildren = Array.from(section.childNodes);
                    return [
                        response.contentType === "text/html",
                        root.parentElement === response.body,
                        rootChildren.length === 2
                            && rootChildren[0] === foster
                            && rootChildren[1] === table,
                        foster.parentElement === root,
                        nested.parentElement === foster,
                        table.children[0] === section,
                        sectionChildren.length === 3
                            && sectionChildren[0].nodeType === 3
                            && sectionChildren[0].nodeValue === "  "
                            && sectionChildren[1].nodeType === 8
                            && sectionChildren[2].localName === "tr",
                    ];
                })()"##,
                &document,
                "fixture://xhr-html-foster.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("XHR HTML response document must parse");

    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true, true, true])
    );
}

#[test]
fn xhr_html_response_document_keeps_disabled_script_and_foreign_tree_semantics() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("xhr-html-standards-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse("<main></main>", &NativeEngineLimits::default()).unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const response = globalThis.__glassParseHtmlDocument(
                        "<!doctype html><body><script>globalThis.__glassXhrResponseScriptRan=true;</script><noscript id='fallback'><b id='fallback-child'>visible</b></noscript><svg viewBox='0 0 1 1'><linearGradient id='gradient' xlink:href='#target'></linearGradient></svg><math><annotation-xml id='math-annotation' encoding='text/html' definitionurl='urn:glass:test'><div id='math-html'>math</div></annotation-xml></math><template id='template'><table><tr><td id='template-cell'>template</td></tr></table></template></body>",
                        "https://example.test/response.html",
                        "text/html; charset=UTF-8",
                    );
                    if (!response) return null;
                    const fallback = response.getElementById("fallback");
                    const fallbackChild = response.getElementById("fallback-child");
                    const svg = response.querySelector("svg");
                    const gradient = response.getElementById("gradient");
                    const xlink = gradient && gradient.getAttributeNode("xlink:href");
                    const annotation = response.getElementById("math-annotation");
                    const mathHtml = response.getElementById("math-html");
                    const template = response.getElementById("template");
                    const templateCell = template.content.getElementById("template-cell");
                    const serialized = new XMLSerializer().serializeToString(response);
                    return [
                        response.compatMode === "CSS1Compat",
                        fallbackChild.parentElement === fallback,
                        globalThis.__glassXhrResponseScriptRan === undefined,
                        gradient.localName === "linearGradient",
                        gradient.namespaceURI === "http://www.w3.org/2000/svg",
                        svg.getAttribute("viewBox") === "0 0 1 1",
                        xlink.namespaceURI === "http://www.w3.org/1999/xlink",
                        annotation.namespaceURI === "http://www.w3.org/1998/Math/MathML",
                        annotation.getAttributeNode("definitionURL").name === "definitionURL",
                        annotation.getAttribute("definitionURL") === "urn:glass:test",
                        mathHtml.namespaceURI === "http://www.w3.org/1999/xhtml"
                            && mathHtml.parentElement === annotation,
                        template instanceof HTMLTemplateElement,
                        template.content instanceof DocumentFragment
                            && template.content.nodeType === 11
                            && template.content.parentNode === null,
                        template.content.ownerDocument !== response
                            && template.content.ownerDocument.contentType === "text/html",
                        template.childNodes.length === 0 && template.children.length === 0,
                        template.querySelector("#template-cell") === null
                            && response.getElementById("template-cell") === null,
                        template.content.querySelector("#template-cell") === templateCell,
                        templateCell.getRootNode() === template.content,
                        templateCell.parentElement.localName === "tr",
                        template.innerHTML.includes("id=\"template-cell\""),
                        serialized.includes("<template id=\"template\"><table>"),
                        serialized.includes(
                            "<linearGradient id=\"gradient\" xlink:href=\"#target\"></linearGradient>",
                        ),
                    ];
                })()"##,
                &document,
                "fixture://xhr-html-standards.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("XHR HTML response document must parse");

    assert_eq!(
        evaluation.value,
        serde_json::json!([
            true, true, true, true, true, true, true, true, true, true, true, true, true, true,
            true, true, true, true, true, true, true, true,
        ])
    );
}

#[test]
fn xhr_html_response_document_accepts_bounded_structured_parser_output() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("xhr-html-result-limit-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse("<main></main>", &NativeEngineLimits::default()).unwrap();
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const source = "<p>x</p>".repeat(1024);
                    const response = globalThis.__glassParseHtmlDocument(
                        source,
                        "https://example.test/large-response.html",
                        "text/html",
                    );
                    return response !== null && response.body.children.length === 1024;
                })()"##,
            &document,
            "fixture://xhr-html-result-limit.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("bounded XHR HTML response document must parse");

    assert_eq!(evaluation.value, serde_json::json!(true));
}

#[test]
fn xhr_html_response_document_implies_table_sections_rows_and_colgroups() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("xhr-implied-table-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse("<main></main>", &NativeEngineLimits::default()).unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const response = globalThis.__glassParseHtmlDocument(
                        "<main id='root'><table id='rows'><tr id='row'><td id='cell'>row</td></tr></table><table id='cells'><td id='direct-cell'>direct</td></table><table id='explicit'><tbody id='body'><tr id='explicit-row'><td>explicit</td></tr></tbody></table><table id='columns'><col id='column'></table><table id='explicit-columns'><colgroup id='explicit-group'><col id='explicit-column'></colgroup></table><svg><table id='svg-table'><tr id='svg-row'><td>svg</td></tr></table><table id='svg-columns'><col id='svg-column'></table><foreignObject><table id='html-table'><tr id='html-row'><td>html</td></tr></table><table id='html-columns'><col id='html-column'></table></foreignObject></svg><math><table id='math-columns'><col id='math-column'></table></math></main>",
                        "https://example.test/response.html",
                        "text/html",
                    );
                    if (!response) return null;
                    const rows = response.getElementById("rows");
                    const row = response.getElementById("row");
                    const body = rows && rows.children[0];
                    const cells = response.getElementById("cells");
                    const directCell = response.getElementById("direct-cell");
                    const explicit = response.getElementById("explicit");
                    const svgTable = response.getElementById("svg-table");
                    const svgRow = response.getElementById("svg-row");
                    const svgColumns = response.getElementById("svg-columns");
                    const svgColumn = response.getElementById("svg-column");
                    const htmlTable = response.getElementById("html-table");
                    const htmlRow = response.getElementById("html-row");
                    const columns = response.getElementById("columns");
                    const column = response.getElementById("column");
                    const explicitColumns = response.getElementById("explicit-columns");
                    const explicitGroup = response.getElementById("explicit-group");
                    const htmlColumns = response.getElementById("html-columns");
                    const htmlColumn = response.getElementById("html-column");
                    const mathColumns = response.getElementById("math-columns");
                    const mathColumn = response.getElementById("math-column");
                    return [
                        body.localName === "tbody" && row.parentElement === body,
                        row.children[0] === response.getElementById("cell"),
                        directCell.parentElement.localName === "tr"
                            && directCell.parentElement.parentElement.localName === "tbody",
                        explicit.children.length === 1
                            && explicit.children[0] === response.getElementById("body"),
                        svgTable.children[0].localName === "tbody"
                            && svgRow.parentElement === svgTable.children[0]
                            && svgRow.namespaceURI === "http://www.w3.org/1999/xhtml",
                        htmlTable.children[0].localName === "tbody"
                            && htmlRow.parentElement === htmlTable.children[0],
                        columns.children[0].localName === "colgroup"
                            && column.parentElement === columns.children[0],
                        explicitColumns.children.length === 1
                            && explicitColumns.children[0] === explicitGroup,
                        svgColumns.children[0].localName === "colgroup"
                            && svgColumn.parentElement === svgColumns.children[0]
                            && svgColumn.namespaceURI === "http://www.w3.org/1999/xhtml",
                        htmlColumns.children[0].localName === "colgroup"
                            && htmlColumn.parentElement === htmlColumns.children[0],
                        mathColumns.children[0].localName === "colgroup"
                            && mathColumn.parentElement === mathColumns.children[0]
                            && mathColumn.namespaceURI === "http://www.w3.org/1999/xhtml",
                    ];
                })()"##,
                &document,
                "fixture://xhr-implied-table.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("XHR HTML document must preserve implied table structure");

    assert_eq!(
        evaluation.value,
        serde_json::json!([
            true, true, true, true, true, true, true, true, true, true, true
        ])
    );
}

#[test]
fn xhr_html_response_document_recovers_nested_table_starts() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("xhr-nested-table-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse("<main></main>", &NativeEngineLimits::default()).unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const response = globalThis.__glassParseHtmlDocument(
                        "<main id='root'><table id='outer'><tbody><tr><td>outer</td></tr><table id='inner'><tr><td>inner</td></tr></table></tbody><table id='cell-table'><tbody><tr><td id='cell'><table id='valid-nested'></table></td></tr></tbody></table></main>",
                        "https://example.test/nested-table.html",
                        "text/html",
                    );
                    if (!response) return null;
                    const root = response.getElementById("root");
                    const outer = response.getElementById("outer");
                    const inner = response.getElementById("inner");
                    const cellTable = response.getElementById("cell-table");
                    const cell = response.getElementById("cell");
                    const validNested = response.getElementById("valid-nested");
                    return [
                        root.children.length === 3
                            && root.children[0] === outer
                            && root.children[1] === inner
                            && root.children[2] === cellTable,
                        outer.parentElement === root && inner.parentElement === root,
                        validNested.parentElement === cell,
                    ];
                })()"##,
                &document,
                "fixture://xhr-nested-table.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("XHR HTML table-start recovery must parse");

    assert_eq!(evaluation.value, serde_json::json!([true, true, true]));
}

#[test]
fn javascript_and_xhr_table_end_tags_honor_scope_and_fragment_context() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("table-end-scope-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
        "<main id='root'></main><table id='fragment-context'></table>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const markup = "<table id='outer'><tbody><tr><td id='cell'><table id='nested'><tbody><tr><td>nested</td></tr></tbody></table><template id='template'><span id='template-before'>before</span></table><span id='template-after'>after</span></template><span id='cell-after'>tail</span></td></tr></tbody></table><p id='outside'>outside</p>";
                    const root = document.querySelector("#root");
                    root.innerHTML = markup;
                    const fragmentContext = document.querySelector("#fragment-context");
                    fragmentContext.innerHTML = "<tbody><tr><td id='fragment-cell'><span id='fragment-before'>before</span></table><span id='fragment-after'>after</span></td></tr></tbody>";
                    const response = globalThis.__glassParseHtmlDocument(
                        "<main id='response-root'>" + markup + "</main>",
                        "https://example.test/table-end-scope.html",
                        "text/html",
                    );
                    if (!response) return null;
                    const responseRoot = response.getElementById("response-root");
                    const responseTemplate = response.getElementById("template");
                    const responseCell = response.getElementById("cell");
                    const rootTemplate = root.querySelector("#template");
                    const responseTemplateAfter = responseTemplate.content.querySelector("#template-after");
                    return [
                        rootTemplate.content.querySelector("#template-after").parentNode === rootTemplate.content,
                        root.querySelector("#cell-after").parentElement === root.querySelector("#cell"),
                        root.querySelector("#nested").parentElement === root.querySelector("#cell"),
                        root.querySelector("#outside").parentElement === root,
                        fragmentContext.querySelector("#fragment-after").parentElement === fragmentContext.querySelector("#fragment-cell"),
                        responseTemplateAfter.parentNode === responseTemplate.content,
                        response.getElementById("cell-after").parentElement === responseCell,
                        response.getElementById("nested").parentElement === responseCell,
                        response.getElementById("outside").parentElement === responseRoot,
                    ];
                })()"##,
                &document,
                "fixture://table-end-scope.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("HTML table end tags must honor table scope");

    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true, true, true, true, true])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("same-turn table fragments must commit through the native parser");

    let root = document.find_element_by_id("root").unwrap();
    let cell = document.find_element_by_id("cell").unwrap();
    let nested = document.find_element_by_id("nested").unwrap();
    let template = document.find_element_by_id("template").unwrap();
    let template_after = find_template_element_by_id(&document, template, "template-after");
    let cell_after = document.find_element_by_id("cell-after").unwrap();
    assert_eq!(document.node(nested).unwrap().parent(), Some(cell));
    assert_eq!(
        document.raw_node(template_after).unwrap().parent(),
        Some(
            document
                .raw_node(template)
                .unwrap()
                .template_content
                .unwrap()
        )
    );
    assert_eq!(document.node(cell_after).unwrap().parent(), Some(cell));
    assert_eq!(document.node(root).unwrap().children().len(), 2);

    let fragment_context = document.find_element_by_id("fragment-context").unwrap();
    let fragment_cell = document.find_element_by_id("fragment-cell").unwrap();
    let fragment_after = document.find_element_by_id("fragment-after").unwrap();
    assert_eq!(
        document.node(fragment_after).unwrap().parent(),
        Some(fragment_cell)
    );
    let fragment_row = document.node(fragment_cell).unwrap().parent().unwrap();
    assert_eq!(
        document
            .node(fragment_row)
            .and_then(NativeNode::element_name),
        Some("tr")
    );
    let fragment_body = document.node(fragment_row).unwrap().parent().unwrap();
    assert_eq!(
        document
            .node(fragment_body)
            .and_then(NativeNode::element_name),
        Some("tbody")
    );
    assert_eq!(
        document.node(fragment_body).unwrap().parent(),
        Some(fragment_context)
    );
}

#[test]
fn javascript_xhr_and_fragments_share_table_structure_end_tag_scope() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("table-structure-end-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
        "<main id='root'></main><table><tbody id='fragment-context'></tbody></table>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const markup = "<div id='host'><table id='table'><tbody id='body'><tr id='row'><td id='cell'><span id='inside'>x</span></tbody><tr id='next-row'><td>y</td></tr></table><svg id='svg'><tbody id='svg-body'><foreignObject><div id='foreign-host'><span>before</span></tbody><span id='foreign-after'>after</span></div></foreignObject></tbody></svg></div>";
                    const root = document.querySelector("#root");
                    root.innerHTML = markup;
                    const fragmentContext = document.querySelector("#fragment-context");
                    fragmentContext.innerHTML = "<tr id='fragment-row-one'><td>one</td></tr></tbody><tr id='fragment-row-two'><td>two</td></tr>";
                    const response = globalThis.__glassParseHtmlDocument(
                        markup,
                        "https://example.test/table-structure-end.html",
                        "text/html",
                    );
                    const validGroupAndForeignScope = (container) => {
                        const table = container.querySelector("#table");
                        const nextRow = container.querySelector("#next-row");
                        const foreignHost = container.querySelector("#foreign-host");
                        return nextRow.parentElement.localName === "tbody"
                            && nextRow.parentElement.parentElement === table
                            && container.querySelector("#foreign-after").parentElement === foreignHost
                            && container.querySelector("#svg-body").namespaceURI === "http://www.w3.org/2000/svg";
                    };
                    return [
                        validGroupAndForeignScope(root),
                        fragmentContext.children.length === 2,
                        fragmentContext.querySelector("#fragment-row-one").parentElement === fragmentContext,
                        fragmentContext.querySelector("#fragment-row-two").parentElement === fragmentContext,
                        validGroupAndForeignScope(response),
                    ];
                })()"##,
                &document,
                "fixture://table-structure-end.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("HTML table row-group and row end tags must honor scope");

    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("same-turn table fragments must commit through the native parser");

    let root = document.find_element_by_id("root").unwrap();
    let table = document.find_element_by_id("table").unwrap();
    let next_row = document.find_element_by_id("next-row").unwrap();
    let foreign_host = document.find_element_by_id("foreign-host").unwrap();
    let foreign_after = document.find_element_by_id("foreign-after").unwrap();
    let foreign_body = document.find_element_by_id("svg-body").unwrap();
    let implied_group = document.node(next_row).unwrap().parent().unwrap();
    assert_eq!(
        document
            .node(implied_group)
            .and_then(NativeNode::element_name),
        Some("tbody")
    );
    assert_eq!(document.node(implied_group).unwrap().parent(), Some(table));
    assert_eq!(
        document.node(foreign_after).unwrap().parent(),
        Some(foreign_host)
    );
    assert_eq!(
        document
            .node(foreign_body)
            .and_then(NativeNode::namespace_uri),
        Some(SVG_NAMESPACE_URI)
    );
    assert_eq!(document.node(root).unwrap().children().len(), 1);

    let fragment_context = document.find_element_by_id("fragment-context").unwrap();
    let first_row = document.find_element_by_id("fragment-row-one").unwrap();
    let second_row = document.find_element_by_id("fragment-row-two").unwrap();
    assert_eq!(
        document.node(first_row).unwrap().parent(),
        Some(fragment_context)
    );
    assert_eq!(
        document.node(second_row).unwrap().parent(),
        Some(fragment_context)
    );
}

#[test]
fn javascript_xhr_and_fragments_share_table_cell_end_tag_scope() {
    const MARKUP: &str = concat!(
        "<table id='cells'><tbody><tr id='cells-row'><td id='data-cell'><span id='data-child'>d</span></td><th id='header-cell'><span id='header-child'>h</span></th></tr></tbody></table>",
        "<table id='outer-table'><tbody><tr><td id='outer-cell'><table id='inner-table'><tbody><tr id='inner-row'></tr></tbody></td><span id='after-inner'>after</span></table></td></tr></tbody></table>",
        "<table id='template-table'><tbody><tr><td id='template-cell'><template id='template'><div id='template-before'>before</div></td><span id='template-after'>inside</span></template><span id='after-template'>after</span></td></tr></tbody></table>",
        "<table id='foreign-table'><tbody><tr><td id='foreign-outer'><svg id='svg'><th id='foreign-th'><foreignObject><div id='foreign-wrapper'><span id='foreign-before'>before</span></th><span id='foreign-after'>after</span></div></foreignObject></th></svg><span id='after-svg'>tail</span></td></tr></tbody></table>"
    );
    let assert_cell_tree = |tree: &NativeDocument| {
        let row = tree.find_element_by_id("cells-row").unwrap();
        for cell_id in ["data-cell", "header-cell"] {
            let cell = tree.find_element_by_id(cell_id).unwrap();
            assert_eq!(tree.node(cell).unwrap().parent(), Some(row));
        }
        let data_cell = tree.find_element_by_id("data-cell").unwrap();
        let data_child = tree.find_element_by_id("data-child").unwrap();
        assert_eq!(tree.node(data_child).unwrap().parent(), Some(data_cell));
        let header_cell = tree.find_element_by_id("header-cell").unwrap();
        let header_child = tree.find_element_by_id("header-child").unwrap();
        assert_eq!(tree.node(header_child).unwrap().parent(), Some(header_cell));

        let outer_cell = tree.find_element_by_id("outer-cell").unwrap();
        let after_inner = tree.find_element_by_id("after-inner").unwrap();
        assert_eq!(tree.node(after_inner).unwrap().parent(), Some(outer_cell));

        let template_cell = tree.find_element_by_id("template-cell").unwrap();
        let template = tree.find_element_by_id("template").unwrap();
        let template_after = find_template_element_by_id(tree, template, "template-after");
        let after_template = tree.find_element_by_id("after-template").unwrap();
        assert_eq!(tree.node(template).unwrap().parent(), Some(template_cell));
        assert_eq!(
            tree.raw_node(template_after).unwrap().parent(),
            Some(tree.raw_node(template).unwrap().template_content.unwrap())
        );
        assert_eq!(
            tree.node(after_template).unwrap().parent(),
            Some(template_cell)
        );

        let foreign_wrapper = tree.find_element_by_id("foreign-wrapper").unwrap();
        let foreign_after = tree.find_element_by_id("foreign-after").unwrap();
        assert_eq!(
            tree.node(foreign_after).unwrap().parent(),
            Some(foreign_wrapper)
        );
        let foreign_th = tree.find_element_by_id("foreign-th").unwrap();
        assert_eq!(
            tree.node(foreign_th).and_then(NativeNode::namespace_uri),
            Some(SVG_NAMESPACE_URI)
        );
        let foreign_outer = tree.find_element_by_id("foreign-outer").unwrap();
        let after_svg = tree.find_element_by_id("after-svg").unwrap();
        assert_eq!(tree.node(after_svg).unwrap().parent(), Some(foreign_outer));
    };

    let direct_document = NativeDocument::parse(MARKUP, &NativeEngineLimits::default())
        .expect("HTML table cell end tags must honor table scope");
    assert_cell_tree(&direct_document);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("table-cell-end-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
            "<main id='root'></main><table><tbody><tr><td id='td-context'></td><th id='th-context'></th></tr></tbody></table>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let script = r##"(() => {
                    const markup = __HTML_MARKUP__;
                    const root = document.querySelector("#root");
                    root.innerHTML = markup;
                    const tdContext = document.querySelector("#td-context");
                    const thContext = document.querySelector("#th-context");
                    tdContext.innerHTML = "</td><span id='fragment-td-child'>td</span>";
                    thContext.innerHTML = "</th><span id='fragment-th-child'>th</span>";
                    const response = globalThis.__glassParseHtmlDocument(
                        markup,
                        "https://example.test/table-cell-end.html",
                        "text/html",
                    );
                    const cellTreeIsScoped = (container) => {
                        const row = container.querySelector("#cells-row");
                        const dataCell = container.querySelector("#data-cell");
                        const headerCell = container.querySelector("#header-cell");
                        const outerCell = container.querySelector("#outer-cell");
                        const templateCell = container.querySelector("#template-cell");
                        const template = container.querySelector("#template");
                        const foreignWrapper = container.querySelector("#foreign-wrapper");
                        const foreignOuter = container.querySelector("#foreign-outer");
                        return dataCell.parentElement === row
                            && headerCell.parentElement === row
                            && container.querySelector("#data-child").parentElement === dataCell
                            && container.querySelector("#header-child").parentElement === headerCell
                            && container.querySelector("#after-inner").parentElement === outerCell
                            && template.parentElement === templateCell
                            && template.content.querySelector("#template-after").parentNode === template.content
                            && container.querySelector("#after-template").parentElement === templateCell
                            && container.querySelector("#foreign-after").parentElement === foreignWrapper
                            && container.querySelector("#foreign-th").namespaceURI === "http://www.w3.org/2000/svg"
                            && container.querySelector("#after-svg").parentElement === foreignOuter;
                    };
                    return [
                        cellTreeIsScoped(root),
                        cellTreeIsScoped(response),
                        tdContext.firstElementChild.id === "fragment-td-child",
                        thContext.firstElementChild.id === "fragment-th-child",
                    ];
                })()"##
            .replace("__HTML_MARKUP__", &serde_json::to_string(MARKUP).unwrap());
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://table-cell-end.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("HTML table cell end tags must honor scope");

    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("same-turn table cell fragments must commit through the native parser");
    assert_cell_tree(&document);

    let td_context = document.find_element_by_id("td-context").unwrap();
    let td_child = document.find_element_by_id("fragment-td-child").unwrap();
    assert_eq!(document.node(td_child).unwrap().parent(), Some(td_context));
    let th_context = document.find_element_by_id("th-context").unwrap();
    let th_child = document.find_element_by_id("fragment-th-child").unwrap();
    assert_eq!(document.node(th_child).unwrap().parent(), Some(th_context));
}

#[test]
fn javascript_xhr_and_fragments_share_table_cell_structural_start_recovery() {
    const MARKUP: &str = concat!(
        "<main id='host'>",
        "<table id='cells-table'><tbody id='cells-body'><tr id='cells-row'>",
        "<td id='first-td'><div id='td-wrapper'><td id='next-td'>next</td></div>",
        "<th id='first-th'><div id='th-wrapper'><th id='next-th'>next</th></div>",
        "</tr></tbody></table>",
        "<table id='rows-table'><tbody id='rows-body'><tr id='first-row'>",
        "<td><div id='row-wrapper'><tr id='next-row'><td>next</td></tr></div>",
        "</tr></tbody></table>",
        "<table id='tbody-table'><tbody id='old-tbody'><tr><td><div>",
        "<tbody id='new-tbody'><tr><td>new</td></tr></tbody>",
        "</div></td></tr></tbody></table>",
        "<table id='thead-table'><tbody id='old-thead-group'><tr><td><div>",
        "<thead id='new-thead'><tr><th>new</th></tr></thead>",
        "</div></td></tr></tbody></table>",
        "<table id='tfoot-table'><tbody id='old-tfoot-group'><tr><td><div>",
        "<tfoot id='new-tfoot'><tr><td>new</td></tr></tfoot>",
        "</div></td></tr></tbody></table>",
        "<table id='caption-table'><tbody><tr><td><div>",
        "<caption id='recovered-caption'>caption</caption>",
        "</div></td></tr></tbody></table>",
        "<table id='colgroup-table'><tbody><tr><td><div>",
        "<colgroup id='recovered-colgroup'><col></colgroup>",
        "</div></td></tr></tbody></table>",
        "<table id='col-table'><tbody><tr><td><div>",
        "<col id='recovered-col'>",
        "</div></td></tr></tbody></table>",
        "<table id='nested-outer-table'><tbody><tr><td id='nested-outer-cell'>",
        "<table id='nested-inner-table'><tbody id='nested-inner-body'><tr><td>inner</td>",
        "<div><tr id='nested-next-row'></tr></div></tr></tbody></table>",
        "<span id='after-inner-table'>outer</span></td></tr></tbody></table>",
        "<table id='template-table'><tbody><tr><td id='template-cell'>",
        "<template id='template-boundary'><div id='template-wrapper'>",
        "<tr id='template-row'></tr></div></template>",
        "<span id='after-template-boundary'>outer</span></td></tr></tbody></table>",
        "<table id='foreign-table'><tbody><tr><td id='foreign-cell'>",
        "<svg><g id='foreign-group'><tr id='foreign-row'></tr></g>",
        "<foreignObject><div id='foreign-object-wrapper'>",
        "<tfoot id='foreign-object-section'></tfoot>",
        "</div></foreignObject></svg><span id='after-foreign'>outer</span>",
        "</td></tr></tbody></table>",
        "<div id='unscoped-host'><td id='unscoped-cell'><div id='unscoped-wrapper'>",
        "<tr id='unscoped-row'></tr></div></td></div>",
        "</main>"
    );
    let assert_parent = |tree: &NativeDocument, child_name: &str, parent_name: &str| {
        let child = tree
            .find_element_by_id(child_name)
            .unwrap_or_else(|| panic!("missing {child_name}"));
        let parent = tree
            .find_element_by_id(parent_name)
            .unwrap_or_else(|| panic!("missing {parent_name}"));
        assert_eq!(
            tree.raw_node(child).and_then(NativeNode::parent),
            Some(parent),
            "unexpected parent for {child_name}"
        );
    };
    let assert_structure = |tree: &NativeDocument| {
        for (child, parent) in [
            ("first-td", "cells-row"),
            ("next-td", "cells-row"),
            ("first-th", "cells-row"),
            ("next-th", "cells-row"),
            ("first-row", "rows-body"),
            ("next-row", "rows-body"),
            ("old-tbody", "tbody-table"),
            ("new-tbody", "tbody-table"),
            ("old-thead-group", "thead-table"),
            ("new-thead", "thead-table"),
            ("old-tfoot-group", "tfoot-table"),
            ("new-tfoot", "tfoot-table"),
            ("recovered-caption", "caption-table"),
            ("recovered-colgroup", "colgroup-table"),
            ("nested-inner-table", "nested-outer-cell"),
            ("after-inner-table", "nested-outer-cell"),
            ("after-template-boundary", "template-cell"),
            ("foreign-row", "foreign-group"),
            ("foreign-object-section", "foreign-table"),
            ("after-foreign", "host"),
        ] {
            assert_parent(tree, child, parent);
        }
        // In the in-body insertion mode, a table row outside any table scope
        // is ignored; it is not repaired into an arbitrary div wrapper.
        assert!(tree.find_element_by_id("unscoped-row").is_none());
        let template = tree.find_element_by_id("template-boundary").unwrap();
        assert!(find_template_element_by_id_if_present(tree, template, "template-row").is_none());
        let col_table = tree.find_element_by_id("col-table").unwrap();
        let recovered_col = tree.find_element_by_id("recovered-col").unwrap();
        let colgroup = tree
            .node(recovered_col)
            .and_then(NativeNode::parent)
            .unwrap();
        assert_eq!(
            tree.node(colgroup).and_then(NativeNode::element_name),
            Some("colgroup")
        );
        assert_eq!(
            tree.node(colgroup).and_then(NativeNode::parent),
            Some(col_table)
        );
        let foreign_row = tree.find_element_by_id("foreign-row").unwrap();
        assert_eq!(
            tree.node(foreign_row).and_then(NativeNode::namespace_uri),
            Some(SVG_NAMESPACE_URI)
        );
    };

    let direct_document = NativeDocument::parse(MARKUP, &NativeEngineLimits::default())
        .expect("document parsing must recover structural starts in HTML cells");
    assert_structure(&direct_document);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("table-cell-start-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
            "<main id='root'></main><table><tbody><tr><td id='fragment-context'></td></tr></tbody></table>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let script = r##"(() => {
                    const markup = __HTML_MARKUP__;
                    const root = document.querySelector("#root");
                    root.innerHTML = markup;
                    const fragmentContext = document.querySelector("#fragment-context");
                    fragmentContext.innerHTML = "<div id='fragment-wrapper'><td id='fragment-cell'>fragment</td></div>";
                    const response = globalThis.__glassParseHtmlDocument(
                        markup,
                        "https://example.test/table-cell-start.html",
                        "text/html",
                    );
                    const structureFailures = (tree) => {
                        const parentIs = (child, parent) => {
                            const templateContent = tree.querySelector("#template-boundary")?.content;
                            const node = child === "template-row"
                                ? templateContent?.querySelector(`#${child}`)
                                : tree.querySelector(`#${child}`);
                            const expectedParent = child === "template-row"
                                ? templateContent?.querySelector(`#${parent}`)
                                : tree.querySelector(`#${parent}`);
                            return node?.parentElement === expectedParent;
                        };
                        const column = tree.querySelector("#recovered-col");
                        const implicitColgroup = column?.parentElement;
                        const foreignRow = tree.querySelector("#foreign-row");
                        return [
                            ["first-td", parentIs("first-td", "cells-row")],
                            ["next-td", parentIs("next-td", "cells-row")],
                            ["first-th", parentIs("first-th", "cells-row")],
                            ["next-th", parentIs("next-th", "cells-row")],
                            ["first-row", parentIs("first-row", "rows-body")],
                            ["next-row", parentIs("next-row", "rows-body")],
                            ["old-tbody", parentIs("old-tbody", "tbody-table")],
                            ["new-tbody", parentIs("new-tbody", "tbody-table")],
                            ["old-thead-group", parentIs("old-thead-group", "thead-table")],
                            ["new-thead", parentIs("new-thead", "thead-table")],
                            ["old-tfoot-group", parentIs("old-tfoot-group", "tfoot-table")],
                            ["new-tfoot", parentIs("new-tfoot", "tfoot-table")],
                            ["recovered-caption", parentIs("recovered-caption", "caption-table")],
                            ["recovered-colgroup", parentIs("recovered-colgroup", "colgroup-table")],
                            ["recovered-col", column?.parentElement === implicitColgroup],
                            ["implicit-colgroup", implicitColgroup?.localName === "colgroup"],
                            ["implicit-colgroup parent", implicitColgroup?.parentElement?.getAttribute("id") === "col-table"],
                            ["nested-inner-table", parentIs("nested-inner-table", "nested-outer-cell")],
                            ["after-inner-table", parentIs("after-inner-table", "nested-outer-cell")],
                            ["template-row outside table scope is ignored", tree.querySelector("#template-boundary")?.content.querySelector("#template-row") === null],
                            ["after-template-boundary", parentIs("after-template-boundary", "template-cell")],
                            ["foreign-row", parentIs("foreign-row", "foreign-group")],
                            ["foreign-row namespace", foreignRow?.namespaceURI === "http://www.w3.org/2000/svg"],
                            ["foreign-object-section", parentIs("foreign-object-section", "foreign-table")],
                            ["after-foreign", parentIs("after-foreign", "host")],
                            ["unscoped-row outside table scope is ignored", tree.querySelector("#unscoped-row") === null],
                        ].filter(([, matches]) => !matches).map(([name]) => name);
                    };
                    return [
                        structureFailures(root),
                        structureFailures(response),
                        [
                            fragmentContext.querySelector("#fragment-cell") === null,
                            fragmentContext.querySelector("#fragment-wrapper")?.localName === "div",
                        ],
                    ];
                })()"##
            .replace("__HTML_MARKUP__", &serde_json::to_string(MARKUP).unwrap());
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://table-cell-start.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("same-turn and XHR parsing must recover structural starts");
    assert_eq!(evaluation.value, serde_json::json!([[], [], [true, true]]));
    document
        .apply_script_commands(&evaluation.commands)
        .expect("fragment commits must use the same structural-start recovery");
    assert_structure(&document);
    let fragment_context = document.find_element_by_id("fragment-context").unwrap();
    let fragment_wrapper = document.find_element_by_id("fragment-wrapper").unwrap();
    assert!(document.find_element_by_id("fragment-cell").is_none());
    assert_eq!(
        document.node(fragment_wrapper).and_then(NativeNode::parent),
        Some(fragment_context)
    );
}

#[test]
fn javascript_xhr_and_fragments_ignore_in_cell_end_tags() {
    let targets = ["body", "caption", "col", "colgroup", "html"];
    let mut markup = String::new();
    for target in targets {
        let (prefix, suffix) = match target {
            "body" => (
                format!("<html id='ancestor-html-{target}'><body id='ancestor-body-{target}'>"),
                "</body></html>".to_owned(),
            ),
            "html" => (
                format!("<html id='ancestor-html-{target}'><body id='ancestor-body-{target}'>"),
                "</body></html>".to_owned(),
            ),
            "caption" | "colgroup" => (
                format!("<{target} id='ancestor-{target}'>"),
                format!("</{target}>"),
            ),
            "col" => (String::new(), String::new()),
            _ => unreachable!("target list is exhaustive"),
        };
        markup.push_str(&prefix);
        markup.push_str(&format!(
                "<table id='table-{target}'><tbody><tr><td id='cell-{target}'><div></div></{target}><span id='after-{target}'></span></td></tr></tbody></table>{suffix}"
            ));
    }

    let assert_parentage = |tree: &NativeDocument| {
        for target in targets {
            let marker = tree
                .find_element_by_id(&format!("after-{target}"))
                .unwrap_or_else(|| panic!("missing marker after </{target}>"));
            let cell = tree
                .find_element_by_id(&format!("cell-{target}"))
                .unwrap_or_else(|| panic!("missing cell for </{target}>"));
            assert_eq!(
                tree.node(marker).and_then(NativeNode::parent),
                Some(cell),
                "</{target}> must not pop the active HTML cell"
            );
        }
    };

    let direct_document = NativeDocument::parse(&markup, &NativeEngineLimits::default())
        .expect("document parser must ignore in-cell end tags");
    assert_parentage(&direct_document);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("table-cell-end-ignore-test")
        .expect("native JavaScript runtime must construct");
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default())
            .expect("fragment host document must parse");
    let script = r##"(() => {
                    const markup = __HTML_MARKUP__;
                    const targets = ["body", "caption", "col", "colgroup", "html"];
                    const root = document.querySelector("#root");
                    root.innerHTML = markup;
                    const response = globalThis.__glassParseHtmlDocument(
                        markup,
                        "https://example.test/table-cell-end-ignore.html",
                        "text/html",
                    );
                    const parentageMatches = (tree) => targets.map((target) => {
                        const marker = tree.querySelector(`#after-${target}`);
                        const cell = tree.querySelector(`#cell-${target}`);
                        return marker?.parentElement === cell;
                    });
                    return [parentageMatches(root), parentageMatches(response)];
                })()"##
        .replace("__HTML_MARKUP__", &serde_json::to_string(&markup).unwrap());
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://table-cell-end-ignore.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("same-turn and XHR parsers must ignore in-cell end tags");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            [true, true, true, true, true],
            [true, true, true, true, true]
        ])
    );

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must ignore in-cell end tags");
    assert_parentage(&document);
}

#[test]
fn javascript_xhr_and_fragments_ignore_row_end_tags_without_cells() {
    let targets = ["body", "caption", "col", "colgroup", "html"];
    let mut markup = String::new();
    let mut expectations = Vec::new();
    for context in ["row", "row-group"] {
        for target in targets {
            let (prefix, suffix) = match target {
                "body" | "html" => (
                    format!(
                        "<html id='ancestor-html-{context}-{target}'><body id='ancestor-body-{context}-{target}'>"
                    ),
                    "</body></html>".to_owned(),
                ),
                "caption" | "colgroup" => (
                    format!("<{target} id='ancestor-{context}-{target}'>"),
                    format!("</{target}>"),
                ),
                "col" => (String::new(), String::new()),
                _ => unreachable!("target list is exhaustive"),
            };
            let context_id = format!("context-{context}-{target}");
            let marker_id = format!("after-{context}-{target}");
            let open_context = if context == "row" {
                format!("<table><tbody><tr id='{context_id}'>")
            } else {
                format!("<table><tbody id='{context_id}'>")
            };
            let close_context = if context == "row" {
                "</tr></tbody></table>"
            } else {
                "</tbody></table>"
            };
            let marker = if context == "row" {
                format!("<td id='{marker_id}'>kept</td>")
            } else {
                format!("<tr id='{marker_id}'></tr>")
            };
            markup.push_str(&prefix);
            markup.push_str(&format!(
                "{open_context}<div></div></{target}>{marker}{close_context}{suffix}"
            ));
            expectations.push((context_id, marker_id));
        }
    }

    let assert_parentage = |tree: &NativeDocument| {
        for (context_id, marker_id) in &expectations {
            let marker = tree
                .find_element_by_id(marker_id)
                .unwrap_or_else(|| panic!("missing row-context marker {marker_id}"));
            let context_node = tree
                .find_element_by_id(context_id)
                .unwrap_or_else(|| panic!("missing active table context {context_id}"));
            assert_eq!(
                tree.node(marker).and_then(NativeNode::parent),
                Some(context_node),
                "ignored end tag must not pop {context_id}"
            );
        }
    };

    let direct_document = NativeDocument::parse(&markup, &NativeEngineLimits::default())
        .expect("document parser must ignore row-context end tags");
    assert_parentage(&direct_document);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("table-row-end-ignore-test")
        .expect("native JavaScript runtime must construct");
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default())
            .expect("fragment host document must parse");
    let script = r##"(() => {
                    const markup = __HTML_MARKUP__;
                    const expectations = __EXPECTATIONS__;
                    const root = document.querySelector("#root");
                    root.innerHTML = markup;
                    const response = globalThis.__glassParseHtmlDocument(
                        markup,
                        "https://example.test/table-row-end-ignore.html",
                        "text/html",
                    );
                    const parentageMatches = (tree) => expectations.map(([contextId, markerId]) => {
                        const marker = tree.querySelector(`#${markerId}`);
                        const context = tree.querySelector(`#${contextId}`);
                        return marker?.parentElement === context;
                    });
                    return [parentageMatches(root), parentageMatches(response)];
                })()"##
        .replace("__HTML_MARKUP__", &serde_json::to_string(&markup).unwrap())
        .replace(
            "__EXPECTATIONS__",
            &serde_json::to_string(&expectations).unwrap(),
        );
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://table-row-end-ignore.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("same-turn and XHR parsers must ignore row-context end tags");
    let expected = vec![true; expectations.len()];
    assert_eq!(
        evaluation.value,
        serde_json::json!([expected, vec![true; expectations.len()]])
    );

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must ignore row-context end tags");
    assert_parentage(&document);
}

#[test]
fn javascript_xhr_and_fragments_follow_table_end_tag_modes() {
    let mut markup = String::from("<html id='outer-html'><body id='outer-body'>");
    let mut parentage_expectations = Vec::new();

    for target in [
        "body", "caption", "col", "colgroup", "html", "tbody", "td", "tfoot", "th", "thead", "tr",
    ] {
        let table_id = format!("table-mode-{target}");
        let marker_id = format!("table-mode-after-{target}");
        markup.push_str(&format!(
            "<table id='{table_id}'><div></div></{target}><tbody id='{marker_id}'></tbody></table>"
        ));
        parentage_expectations.push((table_id, marker_id));
    }

    for target in [
        "body", "col", "colgroup", "html", "tbody", "td", "tfoot", "th", "thead", "tr",
    ] {
        let table_id = format!("caption-table-{target}");
        let caption_id = format!("active-caption-{target}");
        let caption_child_id = format!("caption-child-{target}");
        let after_caption_id = format!("caption-after-{target}");
        markup.push_str(&format!(
                "<table id='{table_id}'><caption id='{caption_id}'><div></div></{target}><span id='{caption_child_id}'></span></caption><tbody id='{after_caption_id}'></tbody></table>"
            ));
        parentage_expectations.push((caption_id, caption_child_id));
        parentage_expectations.push((table_id, after_caption_id));
    }

    let explicit_caption_table = "explicit-caption-table";
    let explicit_caption = "explicit-caption";
    let after_explicit_caption = "after-explicit-caption";
    markup.push_str(&format!(
            "<table id='{explicit_caption_table}'><caption id='{explicit_caption}'><span></span></caption><tbody id='{after_explicit_caption}'></tbody></table>"
        ));
    parentage_expectations.push((
        explicit_caption_table.to_owned(),
        explicit_caption.to_owned(),
    ));
    parentage_expectations.push((
        explicit_caption_table.to_owned(),
        after_explicit_caption.to_owned(),
    ));

    for target in [
        "body", "caption", "colgroup", "html", "tbody", "td", "tfoot", "th", "thead", "tr",
    ] {
        let table_id = format!("column-table-{target}");
        let column_group_id = format!("active-colgroup-{target}");
        let marker_id = format!("column-after-{target}");
        markup.push_str(&format!(
                "<table id='{table_id}'><colgroup id='{column_group_id}'><col></{target}><tbody id='{marker_id}'></tbody></table>"
            ));
        parentage_expectations.push((table_id, marker_id));
    }

    let column_table = "column-table-col";
    let active_column_group = "active-colgroup-col";
    let after_ignored_column_end = "after-ignored-column-end";
    let after_closed_column_group = "after-closed-column-group";
    markup.push_str(&format!(
            "<table id='{column_table}'><colgroup id='{active_column_group}'><col id='before-ignored-column-end'></col><col id='{after_ignored_column_end}'></col></colgroup><tbody id='{after_closed_column_group}'></tbody></table>"
        ));
    parentage_expectations.push((
        active_column_group.to_owned(),
        after_ignored_column_end.to_owned(),
    ));
    parentage_expectations.push((
        column_table.to_owned(),
        after_closed_column_group.to_owned(),
    ));
    markup.push_str("</body></html>");

    let assert_parentage = |tree: &NativeDocument| {
        for (parent_id, child_id) in &parentage_expectations {
            let parent = tree
                .find_element_by_id(parent_id)
                .unwrap_or_else(|| panic!("missing table-mode parent {parent_id}"));
            let child = tree
                .find_element_by_id(child_id)
                .unwrap_or_else(|| panic!("missing table-mode child {child_id}"));
            assert_eq!(
                tree.node(child).and_then(NativeNode::parent),
                Some(parent),
                "table end-tag handling changed parentage for {child_id}"
            );
        }
    };

    let direct_document = NativeDocument::parse(&markup, &NativeEngineLimits::default())
        .expect("document parser must follow table end-tag insertion modes");
    assert_parentage(&direct_document);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("table-end-tag-modes-test")
        .expect("native JavaScript runtime must construct");
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default())
            .expect("fragment host document must parse");
    let script = r##"(() => {
                    const markup = __HTML_MARKUP__;
                    const expectations = __EXPECTATIONS__;
                    const root = document.querySelector("#root");
                    root.innerHTML = markup;
                    const response = globalThis.__glassParseHtmlDocument(
                        markup,
                        "https://example.test/table-end-tag-modes.html",
                        "text/html",
                    );
                    const parentageMatches = (tree) => {
                        const byId = new Map(Array.from(tree.querySelectorAll("[id]"))
                            .map((element) => [element.id, element]));
                        return expectations.map(([parentId, childId]) =>
                            byId.get(childId)?.parentElement === byId.get(parentId));
                    };
                    return [parentageMatches(root), parentageMatches(response)];
                })()"##
        .replace("__HTML_MARKUP__", &serde_json::to_string(&markup).unwrap())
        .replace(
            "__EXPECTATIONS__",
            &serde_json::to_string(&parentage_expectations).unwrap(),
        );
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://table-end-tag-modes.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("same-turn and XHR parsers must follow table end-tag modes");
    let expected = vec![true; parentage_expectations.len()];
    assert_eq!(
        evaluation.value,
        serde_json::json!([expected, vec![true; parentage_expectations.len()]])
    );

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must follow table end-tag modes");
    assert_parentage(&document);
}

#[test]
fn javascript_xhr_and_fragments_follow_active_formatting_adoption() {
    let markup = concat!(
        "<section id='adopt-zone'>",
        "<b data-kind='misnested'>one<p id='adopt-block'>two<span id='adopt-inside'>three</b>four</span>five</p>",
        "<span id='adopt-reopened'>six</span>",
        "</section>",
        "<section id='simple-zone'><p id='simple-p'>one<b id='simple-bold'>two</b>three</p></section>",
        "<section id='marker-zone'><b data-kind='outer'>before<table><tbody><tr><td id='marker-cell'>inside</b><span id='marker-after'>after</span></td></tr></tbody></table></b></section>",
        "<section id='row-marker-zone'><div><b id='row-marker-format'>before<table><tbody><tr><td>cell</tr></tbody></table></div><span id='row-marker-after'>after</span></b></section>",
        "<section id='anchor-zone'><a id='anchor-one'>one<a id='anchor-two'>two</a>three</a><span id='anchor-after'>after</span></section>",
    );

    let assert_native_tree = |tree: &NativeDocument| {
        let parent_id = |child_id: &str| {
            let child = tree
                .find_element_by_id(child_id)
                .unwrap_or_else(|| panic!("missing active-formatting fixture node {child_id}"));
            tree.node(child)
                .and_then(NativeNode::parent)
                .unwrap_or_else(|| panic!("fixture node {child_id} has no parent"))
        };
        let parent_name = |child_id: &str| {
            tree.node(parent_id(child_id))
                .and_then(NativeNode::element_name)
                .unwrap_or_else(|| panic!("fixture parent for {child_id} is not an element"))
        };
        assert_eq!(parent_name("adopt-block"), "section");
        assert_eq!(parent_name("adopt-inside"), "b");
        assert_eq!(parent_name("adopt-reopened"), "section");
        assert_eq!(parent_name("simple-bold"), "p");
        assert_eq!(parent_name("marker-after"), "td");
        assert_eq!(parent_name("row-marker-after"), "b");
        assert_eq!(
            tree.node(parent_id("row-marker-after"))
                .and_then(|node| node.attribute("id")),
            Some("row-marker-format")
        );
        assert_eq!(parent_name("anchor-one"), "section");
        assert_eq!(parent_name("anchor-two"), "section");
        assert_eq!(parent_name("anchor-after"), "section");

        assert_eq!(
            tree.node(parent_id("adopt-inside"))
                .and_then(|node| node.attribute("data-kind")),
            Some("misnested")
        );
        assert_eq!(parent_id("anchor-one"), parent_id("anchor-two"));
        let adopt_zone = tree.find_element_by_id("adopt-zone").unwrap();
        let first_three = tree
            .node(adopt_zone)
            .unwrap()
            .children()
            .iter()
            .take(3)
            .map(|id| {
                tree.node(*id)
                    .and_then(NativeNode::element_name)
                    .expect("adoption-zone child is an element")
            })
            .collect::<Vec<_>>();
        assert_eq!(first_three, ["b", "p", "span"]);
    };

    let direct = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("document parser must repair active formatting elements");
    assert_native_tree(&direct);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("active-formatting-test")
        .expect("native JavaScript runtime must construct");
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default())
            .expect("fragment host document must parse");
    let script = r##"(() => {
                    const markup = __HTML_MARKUP__;
                    const root = document.querySelector("#root");
                    root.innerHTML = markup;
                    const response = globalThis.__glassParseHtmlDocument(
                        markup,
                        "https://example.test/active-formatting.html",
                        "text/html",
                    );
                    const fixtureIds = [
                        "adopt-zone", "adopt-block", "adopt-inside", "adopt-reopened",
                        "simple-bold", "marker-after", "row-marker-after", "anchor-one",
                        "anchor-two", "anchor-after",
                    ];
                    const summarize = (tree) => {
                        const byId = new Map(fixtureIds.map((id) => [id, tree.querySelector(`#${id}`)]));
                        const parentage = fixtureIds.slice(1).map((id) => {
                            const parent = byId.get(id)?.parentElement || null;
                            return [
                                id,
                                parent?.localName || null,
                                parent?.getAttribute("id") || null,
                                parent?.getAttribute("data-kind") || null,
                            ];
                        });
                        const zoneChildren = Array.from(byId.get("adopt-zone")?.children || [])
                            .slice(0, 3).map((element) => element.localName);
                        return [parentage, zoneChildren.join(",")];
                    };
                    return [summarize(root), summarize(response)];
                })()"##
            .replace("__HTML_MARKUP__", &serde_json::to_string(markup).unwrap());
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://active-formatting.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("same-turn and XHR parsers must repair active formatting elements");
    let expected = serde_json::json!([
        [
            [
                ["adopt-block", "section", "adopt-zone", null],
                ["adopt-inside", "b", null, "misnested"],
                ["adopt-reopened", "section", "adopt-zone", null],
                ["simple-bold", "p", "simple-p", null],
                ["marker-after", "td", "marker-cell", null],
                ["row-marker-after", "b", "row-marker-format", null],
                ["anchor-one", "section", "anchor-zone", null],
                ["anchor-two", "section", "anchor-zone", null],
                ["anchor-after", "section", "anchor-zone", null],
            ],
            "b,p,span"
        ],
        [
            [
                ["adopt-block", "section", "adopt-zone", null],
                ["adopt-inside", "b", null, "misnested"],
                ["adopt-reopened", "section", "adopt-zone", null],
                ["simple-bold", "p", "simple-p", null],
                ["marker-after", "td", "marker-cell", null],
                ["row-marker-after", "b", "row-marker-format", null],
                ["anchor-one", "section", "anchor-zone", null],
                ["anchor-two", "section", "anchor-zone", null],
                ["anchor-after", "section", "anchor-zone", null],
            ],
            "b,p,span"
        ]
    ]);
    assert_eq!(evaluation.value, expected);

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must repair active formatting elements");
    assert_native_tree(&document);
    let root = document.find_element_by_id("root").unwrap();
    let committed = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("matching tree must be available for commit comparison");
    let committed_ids = [
        "adopt-block",
        "adopt-inside",
        "adopt-reopened",
        "simple-bold",
        "marker-after",
        "row-marker-after",
        "anchor-one",
        "anchor-two",
        "anchor-after",
    ];
    for child_id in committed_ids {
        let committed_child = committed.find_element_by_id(child_id).unwrap();
        let child_name = committed
            .node(committed_child)
            .unwrap()
            .element_name()
            .unwrap();
        let actual_child = document
            .find_element_by_id(child_id)
            .unwrap_or_else(|| panic!("fragment commit lost {child_id}"));
        let actual_parent = document
            .node(actual_child)
            .and_then(NativeNode::parent)
            .unwrap();
        let expected_parent_name = committed
            .node(
                committed
                    .node(committed_child)
                    .and_then(NativeNode::parent)
                    .unwrap(),
            )
            .and_then(NativeNode::element_name)
            .unwrap();
        assert_eq!(
            document
                .node(actual_parent)
                .and_then(NativeNode::element_name),
            Some(expected_parent_name),
            "fragment parent mismatch for {child_name}#{child_id}"
        );
    }
    assert!(document.node(root).is_some());
}

#[test]
fn implied_end_tags_close_common_list_option_table_and_paragraph_items() {
    let document = NativeDocument::parse(
        "<ul><li>one<li>two</ul><select><option>one<option>two</select><p>first<div>second</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let list_items = document
        .nodes
        .iter()
        .filter(|node| node.element_name() == Some("li"))
        .collect::<Vec<_>>();
    assert_eq!(list_items.len(), 2);
    assert!(list_items.iter().all(|node| node.parent().is_some()));
    let options = document
        .nodes
        .iter()
        .filter(|node| node.element_name() == Some("option"))
        .collect::<Vec<_>>();
    assert_eq!(options.len(), 2);
    assert!(options.iter().all(|node| node.parent().is_some()));
    let paragraphs = document
        .nodes
        .iter()
        .filter(|node| node.element_name() == Some("p"))
        .collect::<Vec<_>>();
    assert_eq!(paragraphs.len(), 1);
    assert_eq!(
        document.visible_text(1024).0,
        "one two one two first second"
    );
}

#[test]
fn html_parser_implies_table_sections_and_rows_for_missing_containers() {
    let document = NativeDocument::parse(
            "<main><table id='rows'><tr id='row'><td id='cell'>row</td></tr></table><table id='cells'><td id='direct-cell'>direct</td></table><table id='section-cells'><tbody id='section'><th id='section-cell'>section</th></tbody></table><table id='explicit'><tbody id='explicit-body'><tr id='explicit-row'><td>explicit</td></tr></tbody></table></main>",
            &NativeEngineLimits::default(),
        )
        .unwrap();

    let rows = document.find_element_by_id("rows").unwrap();
    let row = document.find_element_by_id("row").unwrap();
    let cell = document.find_element_by_id("cell").unwrap();
    let implied_body = document.node(rows).unwrap().children()[0];
    assert_eq!(
        document
            .node(implied_body)
            .and_then(NativeNode::element_name),
        Some("tbody")
    );
    assert_eq!(document.node(implied_body).unwrap().children(), &[row]);
    assert_eq!(document.node(row).unwrap().parent(), Some(implied_body));
    assert_eq!(document.node(cell).unwrap().parent(), Some(row));

    let cells = document.find_element_by_id("cells").unwrap();
    let direct_cell = document.find_element_by_id("direct-cell").unwrap();
    let implied_body = document.node(cells).unwrap().children()[0];
    let implied_row = document.node(implied_body).unwrap().children()[0];
    assert_eq!(
        document
            .node(implied_body)
            .and_then(NativeNode::element_name),
        Some("tbody")
    );
    assert_eq!(
        document
            .node(implied_row)
            .and_then(NativeNode::element_name),
        Some("tr")
    );
    assert_eq!(
        document.node(direct_cell).unwrap().parent(),
        Some(implied_row)
    );

    let section = document.find_element_by_id("section").unwrap();
    let section_cell = document.find_element_by_id("section-cell").unwrap();
    let implied_row = document.node(section).unwrap().children()[0];
    assert_eq!(
        document
            .node(implied_row)
            .and_then(NativeNode::element_name),
        Some("tr")
    );
    assert_eq!(
        document.node(section_cell).unwrap().parent(),
        Some(implied_row)
    );

    let explicit = document.find_element_by_id("explicit").unwrap();
    let explicit_body = document.find_element_by_id("explicit-body").unwrap();
    let explicit_row = document.find_element_by_id("explicit-row").unwrap();
    assert_eq!(
        document.node(explicit).unwrap().children(),
        &[explicit_body]
    );
    assert_eq!(
        document.node(explicit_body).unwrap().children(),
        &[explicit_row]
    );
}

#[test]
fn html_parser_implies_colgroup_for_direct_columns_only_in_html() {
    let document = NativeDocument::parse(
            "<table id='columns'><col id='first'><col id='second'></table><table id='explicit'><colgroup id='explicit-group'><col id='explicit-column'></colgroup></table><svg><table id='svg-table'><col id='svg-column'></table><foreignObject><table id='html-table'><col id='html-column'></table></foreignObject></svg><math><table id='math-table'><col id='math-column'></table></math>",
            &NativeEngineLimits::default(),
        )
        .unwrap();

    let columns = document.find_element_by_id("columns").unwrap();
    let first = document.find_element_by_id("first").unwrap();
    let second = document.find_element_by_id("second").unwrap();
    let implied_group = document.node(columns).unwrap().children()[0];
    assert_eq!(
        document
            .node(implied_group)
            .and_then(NativeNode::element_name),
        Some("colgroup")
    );
    assert_eq!(
        document.node(implied_group).unwrap().children(),
        &[first, second]
    );

    let explicit = document.find_element_by_id("explicit").unwrap();
    let explicit_group = document.find_element_by_id("explicit-group").unwrap();
    let explicit_column = document.find_element_by_id("explicit-column").unwrap();
    assert_eq!(
        document.node(explicit).unwrap().children(),
        &[explicit_group]
    );
    assert_eq!(
        document.node(explicit_group).unwrap().children(),
        &[explicit_column]
    );

    for (table_id, column_id) in [
        ("svg-table", "svg-column"),
        ("math-table", "math-column"),
        ("html-table", "html-column"),
    ] {
        let table = document.find_element_by_id(table_id).unwrap();
        let column = document.find_element_by_id(column_id).unwrap();
        assert_eq!(
            document.node(table).and_then(NativeNode::namespace_uri),
            Some(HTML_NAMESPACE_URI)
        );
        let group = document.node(table).unwrap().children()[0];
        assert_eq!(
            document.node(group).and_then(NativeNode::element_name),
            Some("colgroup")
        );
        assert_eq!(document.node(group).unwrap().children(), &[column]);
        assert_eq!(
            document.node(column).and_then(NativeNode::namespace_uri),
            Some(HTML_NAMESPACE_URI)
        );
    }
}

#[test]
fn html_parser_reprocesses_nested_table_starts_with_scope_boundaries() {
    let document = NativeDocument::parse(
            "<main id='host'><table id='outer'><tbody><tr><td>outer</td></tr><table id='middle'><tr><td>middle</td></tr><table id='inner'><tr><td>inner</td></tr></table></table></tbody><table id='cell-table'><tbody><tr><td id='cell'>before<table id='valid-nested'><tr><td>nested</td></tr></table>after</td></tr></tbody></table><table id='caption-table'><caption id='caption'><table id='caption-nested'></table></caption></table><table id='template-outer'><tbody><template id='template'><table id='template-table'></table></template></tbody></table><svg id='svg'><area id='svg-area'><g id='svg-area-child'></g></area></svg></main>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let host = document.find_element_by_id("host").unwrap();
    let outer = document.find_element_by_id("outer").unwrap();
    let middle = document.find_element_by_id("middle").unwrap();
    let inner = document.find_element_by_id("inner").unwrap();
    let cell_table = document.find_element_by_id("cell-table").unwrap();
    let caption_table = document.find_element_by_id("caption-table").unwrap();
    let template_outer = document.find_element_by_id("template-outer").unwrap();
    let svg = document.find_element_by_id("svg").unwrap();
    assert_eq!(
        document.node(host).unwrap().children(),
        &[
            outer,
            middle,
            inner,
            cell_table,
            caption_table,
            template_outer,
            svg,
        ]
    );

    let cell = document.find_element_by_id("cell").unwrap();
    let valid_nested = document.find_element_by_id("valid-nested").unwrap();
    assert_eq!(document.node(valid_nested).unwrap().parent(), Some(cell));
    let caption = document.find_element_by_id("caption").unwrap();
    let caption_nested = document.find_element_by_id("caption-nested").unwrap();
    assert_eq!(
        document.node(caption_nested).unwrap().parent(),
        Some(caption)
    );
    let template = document.find_element_by_id("template").unwrap();
    let template_table = find_template_element_by_id(&document, template, "template-table");
    let template_contents = document
        .raw_node(template)
        .unwrap()
        .template_content
        .unwrap();
    assert_eq!(
        document.raw_node(template_table).unwrap().parent(),
        Some(template_contents)
    );
    let svg_area = document.find_element_by_id("svg-area").unwrap();
    let svg_area_child = document.find_element_by_id("svg-area-child").unwrap();
    assert_eq!(
        document.node(svg_area_child).unwrap().parent(),
        Some(svg_area)
    );
    assert_eq!(
        document.node(svg_area).and_then(NativeNode::namespace_uri),
        Some(SVG_NAMESPACE_URI)
    );
    assert_eq!(
        document
            .node(svg_area_child)
            .and_then(NativeNode::namespace_uri),
        Some(SVG_NAMESPACE_URI)
    );
}

#[test]
fn html_table_end_tag_stops_at_template_scope_boundary() {
    let document = NativeDocument::parse(
            "<main id='host'><table id='outer'><tbody><tr><td id='cell'><table id='nested'><tbody><tr><td>nested</td></tr></tbody></table><template id='template'><span id='template-before'>before</span></table><span id='template-after'>after</span></template><span id='cell-after'>tail</span></td></tr></tbody></table><p id='outside'>outside</p></main>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let host = document.find_element_by_id("host").unwrap();
    let outer = document.find_element_by_id("outer").unwrap();
    let cell = document.find_element_by_id("cell").unwrap();
    let nested = document.find_element_by_id("nested").unwrap();
    let template = document.find_element_by_id("template").unwrap();
    let template_before = find_template_element_by_id(&document, template, "template-before");
    let template_after = find_template_element_by_id(&document, template, "template-after");
    let cell_after = document.find_element_by_id("cell-after").unwrap();
    let outside = document.find_element_by_id("outside").unwrap();

    assert_eq!(document.node(outer).unwrap().parent(), Some(host));
    assert_eq!(document.node(nested).unwrap().parent(), Some(cell));
    assert_eq!(document.node(template).unwrap().parent(), Some(cell));
    let template_contents = document
        .raw_node(template)
        .unwrap()
        .template_content
        .unwrap();
    assert_eq!(
        document.raw_node(template_before).unwrap().parent(),
        Some(template_contents)
    );
    assert_eq!(
        document.raw_node(template_after).unwrap().parent(),
        Some(template_contents)
    );
    assert_eq!(document.node(cell_after).unwrap().parent(), Some(cell));
    assert_eq!(document.node(outside).unwrap().parent(), Some(host));
}

#[test]
fn html_table_row_group_end_tags_stop_at_nested_table_scope() {
    let document = NativeDocument::parse(
            "<main id='host'><table id='groups'><thead id='head'><tr><th>h</th></tr></thead><tbody id='body'><tr><td>b</td></tr></tbody><tfoot id='foot'><tr><td>f</td></tr></tfoot></table><table id='outer'><tfoot id='outer-foot'><tr id='outer-row'><td id='outer-cell'><table id='inner'><tbody id='inner-body'><tr id='inner-row'><td id='inner-cell'><span id='before'>before</span></tfoot><span id='after'>after</span></td></tr></tbody></table><template id='template'><span id='template-before'>before</span></tfoot><span id='template-after'>after</span></template><span id='outer-after'>tail</span></td></tr></tfoot></table></main>",
            &NativeEngineLimits::default(),
        )
        .unwrap();

    let groups = document.find_element_by_id("groups").unwrap();
    for group_id in ["head", "body", "foot"] {
        let group = document.find_element_by_id(group_id).unwrap();
        assert_eq!(document.node(group).unwrap().parent(), Some(groups));
    }
    let outer = document.find_element_by_id("outer").unwrap();
    let outer_foot = document.find_element_by_id("outer-foot").unwrap();
    let outer_cell = document.find_element_by_id("outer-cell").unwrap();
    let inner = document.find_element_by_id("inner").unwrap();
    let inner_cell = document.find_element_by_id("inner-cell").unwrap();
    let template = document.find_element_by_id("template").unwrap();
    let template_after = find_template_element_by_id(&document, template, "template-after");
    let before = document.find_element_by_id("before").unwrap();
    let after = document.find_element_by_id("after").unwrap();
    let outer_after = document.find_element_by_id("outer-after").unwrap();

    assert_eq!(document.node(outer_foot).unwrap().parent(), Some(outer));
    assert_eq!(document.node(inner).unwrap().parent(), Some(outer_cell));
    assert_eq!(document.node(before).unwrap().parent(), Some(inner_cell));
    assert_eq!(document.node(after).unwrap().parent(), Some(inner_cell));
    assert_eq!(
        document.raw_node(template_after).unwrap().parent(),
        Some(
            document
                .raw_node(template)
                .unwrap()
                .template_content
                .unwrap()
        )
    );
    assert_eq!(
        document.node(outer_after).unwrap().parent(),
        Some(outer_cell)
    );
}

#[test]
fn implied_table_containers_obey_node_and_depth_limits() {
    let node_limited = NativeEngineLimits {
        max_nodes: 3,
        ..NativeEngineLimits::default()
    };
    assert!(matches!(
        NativeDocument::parse("<table><tr><td>cell</td></tr></table>", &node_limited),
        Err(NativeEngineError::LimitExceeded { resource, .. })
            if resource == "DOM nodes" || resource == "temporary HTML parser nodes"
    ));

    let depth_limited = NativeEngineLimits {
        max_dom_depth: 1,
        ..NativeEngineLimits::default()
    };
    assert!(matches!(
        NativeDocument::parse("<table><tr></tr></table>", &depth_limited),
        Err(NativeEngineError::LimitExceeded { resource, .. }) if resource == "DOM depth"
    ));
}

#[test]
fn table_foster_parenting_preserves_whitespace_and_element_descendants() {
    let document = NativeDocument::parse(
            "<main id='host'><p id='before'>before</p><table id='table'> \n&#32;<!-- keep -->alpha <div id='misnested'><b id='inside'>inside</b></div>tail \t<input type='HIDDEN' id='hidden'><tr id='row'><td id='cell'>cell</td></tr></table><p id='after'>after</p></main>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let host = document.find_element_by_id("host").unwrap();
    let table = document.find_element_by_id("table").unwrap();
    let before = document.find_element_by_id("before").unwrap();
    let misnested = document.find_element_by_id("misnested").unwrap();
    let inside = document.find_element_by_id("inside").unwrap();
    let after = document.find_element_by_id("after").unwrap();
    let hidden = document.find_element_by_id("hidden").unwrap();
    let row = document.find_element_by_id("row").unwrap();
    let cell = document.find_element_by_id("cell").unwrap();
    let children = document.node(host).unwrap().children();

    assert_eq!(document.node(table).unwrap().parent(), Some(host));
    assert_eq!(document.node(misnested).unwrap().parent(), Some(host));
    assert_eq!(document.node(inside).unwrap().parent(), Some(misnested));
    assert_eq!(document.node(before).unwrap().parent(), Some(host));
    assert_eq!(document.node(after).unwrap().parent(), Some(host));
    assert_eq!(document.node(hidden).unwrap().parent(), Some(table));
    let table_body = document
        .node(table)
        .unwrap()
        .children()
        .iter()
        .copied()
        .find(|child| document.node(*child).and_then(NativeNode::element_name) == Some("tbody"))
        .unwrap();
    assert_eq!(
        document.node(table_body).and_then(NativeNode::element_name),
        Some("tbody")
    );
    assert_eq!(document.node(row).unwrap().parent(), Some(table_body));
    assert_eq!(document.node(cell).unwrap().parent(), Some(row));
    assert_eq!(children.len(), 6);
    assert_eq!(children[0], before);
    assert_eq!(children[2], misnested);
    assert_eq!(children[4], table);
    assert_eq!(children[5], after);
    assert!(matches!(
        document.node(children[1]).map(NativeNode::kind),
        Some(NativeNodeKind::Text(value)) if value == "alpha "
    ));
    assert!(matches!(
        document.node(children[3]).map(NativeNode::kind),
        Some(NativeNodeKind::Text(value)) if value == "tail \t"
    ));
    let table_children = document.node(table).unwrap().children();
    assert!(matches!(
        table_children.first().and_then(|id| document.node(*id)).map(NativeNode::kind),
        Some(NativeNodeKind::Text(value)) if value == " \n "
    ));
    assert!(matches!(
        table_children.get(1).and_then(|id| document.node(*id)).map(NativeNode::kind),
        Some(NativeNodeKind::Comment(value)) if value == " keep "
    ));
}

#[test]
fn table_foster_parenting_applies_to_section_and_row_contexts() {
    for section in ["tbody", "tfoot", "thead", "tr"] {
        let source = format!(
            "<main id='host'><table id='table'><{section}>misnested</{section}></table></main>"
        );
        let document = NativeDocument::parse(&source, &NativeEngineLimits::default()).unwrap();
        let host = document.find_element_by_id("host").unwrap();
        let table = document.find_element_by_id("table").unwrap();
        let host_children = document.node(host).unwrap().children();
        let table_position = host_children
            .iter()
            .position(|child| *child == table)
            .unwrap();
        assert!(
            table_position > 0,
            "table must follow fostered text for {section}"
        );
        assert!(matches!(
            document
                .node(host_children[table_position - 1])
                .map(NativeNode::kind),
            Some(NativeNodeKind::Text(value)) if value == "misnested"
        ));
        assert_eq!(document.node(table).unwrap().parent(), Some(host));
    }
}

#[test]
fn table_foster_parenting_respects_html_namespaces() {
    let document = NativeDocument::parse(
        "<svg><table id='table'><div id='child'>content</div></table></svg>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let table = document.find_element_by_id("table").unwrap();
    let child = document.find_element_by_id("child").unwrap();
    assert_eq!(
        document.node(table).and_then(NativeNode::namespace_uri),
        Some(HTML_NAMESPACE_URI)
    );
    assert_eq!(
        document.node(child).and_then(NativeNode::namespace_uri),
        Some(HTML_NAMESPACE_URI)
    );
    let parent = document.node(table).unwrap().parent().unwrap();
    assert_eq!(document.node(child).unwrap().parent(), Some(parent));
    let children = document.node(parent).unwrap().children();
    assert!(
        children.iter().position(|child_id| *child_id == child)
            < children.iter().position(|child_id| *child_id == table)
    );

    let document = NativeDocument::parse(
            "<svg><foreignObject><main id='host'><table id='html-table'><div id='fostered'>content</div></table></main></foreignObject></svg>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let host = document.find_element_by_id("host").unwrap();
    let table = document.find_element_by_id("html-table").unwrap();
    let fostered = document.find_element_by_id("fostered").unwrap();
    assert_eq!(
        document.node(table).and_then(NativeNode::namespace_uri),
        Some(HTML_NAMESPACE_URI)
    );
    assert_eq!(document.node(fostered).unwrap().parent(), Some(host));
}

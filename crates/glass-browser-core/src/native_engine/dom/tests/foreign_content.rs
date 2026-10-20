use super::*;

#[test]
fn html_integration_points_select_child_namespaces_across_parser_routes() {
    let markup = concat!(
        "<section id='zone'><svg id='svg-root'>",
        "<desc id='svg-desc'><span id='svg-desc-child'></span><math id='svg-desc-math'><mi id='svg-desc-math-child'></mi></math></desc>",
        "<foreignObject id='svg-foreignobject'><div id='svg-fo-child'/><span id='svg-fo-grandchild'></span></div><svg id='svg-fo-svg'></svg><math id='svg-fo-math'></math></foreignObject>",
        "<g id='svg-g'><area id='svg-g-child'></area><math id='svg-g-math'></math></g>",
        "</svg><math id='math-root'>",
        "<mi id='math-mi'><span id='math-mi-span'></span><mglyph id='math-mi-glyph'/><malignmark id='math-mi-malignmark'/><svg id='math-mi-svg'></svg><math id='math-mi-math'></math></mi>",
        "<mtext id='math-mtext'><strong id='math-mtext-strong'></strong></mtext>",
        "<annotation-xml id='math-text-html-ann' encoding='TEXT/HTML'><span id='annotation-text-html-span'></span></annotation-xml>",
        "<annotation-xml id='math-html-ann' encoding='APPLICATION/XHTML+XML'><span id='annotation-html-span'></span><svg id='annotation-html-svg'></svg></annotation-xml>",
        "<annotation-xml id='math-plain-ann' encoding='application/xml'><area id='annotation-foreign-span'></area><svg id='annotation-foreign-svg'></svg></annotation-xml>",
        "<mrow id='math-foreign'><area id='math-foreign-span'></area><svg id='math-foreign-svg'></svg></mrow>",
        "</math></section>",
    );
    let expected_nodes = [
        ("svg-root", SVG_NAMESPACE_URI, "zone"),
        ("svg-desc", SVG_NAMESPACE_URI, "svg-root"),
        ("svg-desc-child", HTML_NAMESPACE_URI, "svg-desc"),
        ("svg-desc-math", MATHML_NAMESPACE_URI, "svg-desc"),
        ("svg-desc-math-child", MATHML_NAMESPACE_URI, "svg-desc-math"),
        ("svg-foreignobject", SVG_NAMESPACE_URI, "svg-root"),
        ("svg-fo-child", HTML_NAMESPACE_URI, "svg-foreignobject"),
        ("svg-fo-grandchild", HTML_NAMESPACE_URI, "svg-fo-child"),
        ("svg-fo-svg", SVG_NAMESPACE_URI, "svg-foreignobject"),
        ("svg-fo-math", MATHML_NAMESPACE_URI, "svg-foreignobject"),
        ("svg-g", SVG_NAMESPACE_URI, "svg-root"),
        ("svg-g-child", SVG_NAMESPACE_URI, "svg-g"),
        ("svg-g-math", SVG_NAMESPACE_URI, "svg-g"),
        ("math-root", MATHML_NAMESPACE_URI, "zone"),
        ("math-mi", MATHML_NAMESPACE_URI, "math-root"),
        ("math-mi-span", HTML_NAMESPACE_URI, "math-mi"),
        ("math-mi-glyph", MATHML_NAMESPACE_URI, "math-mi"),
        ("math-mi-malignmark", MATHML_NAMESPACE_URI, "math-mi"),
        ("math-mi-svg", SVG_NAMESPACE_URI, "math-mi"),
        ("math-mi-math", MATHML_NAMESPACE_URI, "math-mi"),
        ("math-mtext", MATHML_NAMESPACE_URI, "math-root"),
        ("math-mtext-strong", HTML_NAMESPACE_URI, "math-mtext"),
        ("math-text-html-ann", MATHML_NAMESPACE_URI, "math-root"),
        (
            "annotation-text-html-span",
            HTML_NAMESPACE_URI,
            "math-text-html-ann",
        ),
        ("math-html-ann", MATHML_NAMESPACE_URI, "math-root"),
        ("annotation-html-span", HTML_NAMESPACE_URI, "math-html-ann"),
        ("annotation-html-svg", SVG_NAMESPACE_URI, "math-html-ann"),
        ("math-plain-ann", MATHML_NAMESPACE_URI, "math-root"),
        (
            "annotation-foreign-span",
            MATHML_NAMESPACE_URI,
            "math-plain-ann",
        ),
        (
            "annotation-foreign-svg",
            SVG_NAMESPACE_URI,
            "math-plain-ann",
        ),
        ("math-foreign", MATHML_NAMESPACE_URI, "math-root"),
        ("math-foreign-span", MATHML_NAMESPACE_URI, "math-foreign"),
        ("math-foreign-svg", MATHML_NAMESPACE_URI, "math-foreign"),
    ];
    let expected = expected_nodes
        .iter()
        .map(|(_, namespace, parent)| serde_json::json!([namespace, parent]))
        .collect::<Vec<_>>();
    let summarize_native = |tree: &NativeDocument| {
        expected_nodes
            .iter()
            .map(|(element_id, _, _)| {
                let element = tree.find_element_by_id(element_id).unwrap_or_else(|| {
                    panic!("integration fixture element {element_id} must exist")
                });
                let node = tree
                    .node(element)
                    .expect("integration fixture element must resolve");
                let parent_id = node
                    .parent()
                    .and_then(|parent| tree.node(parent))
                    .and_then(|parent| parent.attribute("id"))
                    .expect("integration fixture parent must have an id");
                serde_json::json!([node.namespace_uri().unwrap(), parent_id])
            })
            .collect::<Vec<_>>()
    };

    let direct = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("document parser must select integration-point namespaces");
    assert_eq!(summarize_native(&direct), expected);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("html-integration-point-test")
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
                "https://example.test/html-integration-points.html",
                "text/html",
            );
            const ids = __ELEMENT_IDS__;
            const summarize = (tree) => ids.map((id) => {
                const element = tree.querySelector(`#${id}`);
                return [element.namespaceURI, element.parentElement.getAttribute("id")];
            });
            return [summarize(root), summarize(response)];
        })()"##
        .replace("__HTML_MARKUP__", &serde_json::to_string(markup).unwrap())
        .replace(
            "__ELEMENT_IDS__",
            &serde_json::to_string(
                &expected_nodes
                    .iter()
                    .map(|(element_id, _, _)| element_id)
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://html-integration-points.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("fragment projection and XHR document must parse integration points");
    assert_eq!(evaluation.value, serde_json::json!([expected, expected]));

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must select integration-point namespaces");
    assert_eq!(summarize_native(&document), expected);
}

#[test]
fn foreign_content_breakout_start_tag_set_matches_html_rules() {
    let names = [
        "b",
        "big",
        "blockquote",
        "body",
        "br",
        "center",
        "code",
        "dd",
        "div",
        "dl",
        "dt",
        "em",
        "embed",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "hr",
        "i",
        "img",
        "li",
        "listing",
        "menu",
        "meta",
        "nobr",
        "ol",
        "p",
        "pre",
        "ruby",
        "s",
        "small",
        "span",
        "strong",
        "strike",
        "sub",
        "sup",
        "table",
        "tt",
        "u",
        "ul",
        "var",
    ];
    for name in names {
        assert!(
            foreign_content_breakout_start_tag(name, &BTreeMap::new()),
            "{name} must break out of foreign content"
        );
    }
    assert!(!foreign_content_breakout_start_tag(
        "area",
        &BTreeMap::new()
    ));
    assert!(!foreign_content_breakout_start_tag(
        "font",
        &BTreeMap::new()
    ));
    assert!(!foreign_content_breakout_start_tag(
        "font",
        &BTreeMap::from([("style".to_owned(), "color:red".to_owned())])
    ));
    for name in ["color", "face", "size"] {
        assert!(foreign_content_breakout_start_tag(
            "font",
            &BTreeMap::from([(name.to_owned(), String::new())])
        ));
    }
}

#[test]
fn foreign_content_breakout_start_tags_reprocess_as_html_across_parser_routes() {
    let markup = concat!(
        "<section id='zone'>",
        "<svg id='svg-area-root'><g id='svg-area-parent'>",
        "<area id='svg-area'><g id='svg-area-child'></g></area></g></svg>",
        "<svg id='font-root'><g id='font-no-trigger-parent'>",
        "<font id='font-no-trigger'><g id='font-no-trigger-child'></g></font>",
        "</g></svg>",
        "<svg id='font-break-root'><g id='font-break-parent'>",
        "<font id='font-trigger' color='red'><span id='font-trigger-child'>child</span></font>",
        "</g></svg>",
        "<svg id='svg-div-root'><g id='svg-div-parent'>",
        "<div id='svg-div'/><span id='svg-div-child'>child</span></div>",
        "</g></svg>",
        "<math id='math-root'><mrow id='math-parent'>",
        "<div id='math-div'><span id='math-div-child'>child</span></div>",
        "</mrow></math>",
        "<div id='html-div'/><span id='html-div-child'>child</span>",
        "<svg id='svg-fo-root'><foreignObject id='svg-fo'>",
        "<div id='svg-fo-child'/><span id='svg-fo-grandchild'></span></div></foreignObject></svg>",
        "<math id='math-integration-root'><mi id='math-mi'>",
        "<span id='math-mi-child'></span></mi></math>",
        "</section>",
    );
    let expected_nodes = [
        ("svg-area", SVG_NAMESPACE_URI, "svg-area-parent"),
        ("svg-area-child", SVG_NAMESPACE_URI, "svg-area"),
        (
            "font-no-trigger",
            SVG_NAMESPACE_URI,
            "font-no-trigger-parent",
        ),
        (
            "font-no-trigger-child",
            SVG_NAMESPACE_URI,
            "font-no-trigger",
        ),
        ("font-trigger", HTML_NAMESPACE_URI, "zone"),
        ("font-trigger-child", HTML_NAMESPACE_URI, "font-trigger"),
        ("svg-div", HTML_NAMESPACE_URI, "zone"),
        ("svg-div-child", HTML_NAMESPACE_URI, "svg-div"),
        ("math-div", HTML_NAMESPACE_URI, "zone"),
        ("math-div-child", HTML_NAMESPACE_URI, "math-div"),
        ("html-div", HTML_NAMESPACE_URI, "zone"),
        ("html-div-child", HTML_NAMESPACE_URI, "html-div"),
        ("svg-fo-child", HTML_NAMESPACE_URI, "svg-fo"),
        ("svg-fo-grandchild", HTML_NAMESPACE_URI, "svg-fo-child"),
        ("math-mi-child", HTML_NAMESPACE_URI, "math-mi"),
    ];
    let expected = expected_nodes
        .iter()
        .map(|(_, namespace, parent)| serde_json::json!([namespace, parent]))
        .collect::<Vec<_>>();
    let summarize_native = |tree: &NativeDocument| {
        expected_nodes
            .iter()
            .map(|(element_id, _, _)| {
                let element = tree.find_element_by_id(element_id).unwrap_or_else(|| {
                    panic!("foreign-breakout fixture element {element_id} must exist")
                });
                let node = tree
                    .node(element)
                    .expect("foreign-breakout fixture element must resolve");
                let parent_id = node
                    .parent()
                    .and_then(|parent| tree.node(parent))
                    .and_then(|parent| parent.attribute("id"))
                    .expect("foreign-breakout fixture parent must have an id");
                serde_json::json!([node.namespace_uri().unwrap(), parent_id])
            })
            .collect::<Vec<_>>()
    };

    let direct = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("document parser must reprocess foreign breakout tags as HTML");
    assert_eq!(summarize_native(&direct), expected);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("foreign-breakout-context")
        .expect("native JavaScript runtime must construct");
    let mut projected =
        NativeDocument::parse("<main id='host'></main>", &NativeEngineLimits::default())
            .expect("fragment host document must parse");
    let script = r##"(() => {
            const markup = __HTML_MARKUP__;
            const ids = __ELEMENT_IDS__;
            const host = document.querySelector("#host");
            host.innerHTML = markup;
            const response = globalThis.__glassParseHtmlDocument(
                markup,
                "https://example.test/foreign-content-breakout.html",
                "text/html",
            );
            const summarize = (tree) => ids.map((id) => {
                const element = tree.querySelector(`#${id}`);
                return [element.namespaceURI, element.parentElement.getAttribute("id")];
            });
            return [summarize(host), summarize(response)];
        })()"##
        .replace("__HTML_MARKUP__", &serde_json::to_string(markup).unwrap())
        .replace(
            "__ELEMENT_IDS__",
            &serde_json::to_string(
                &expected_nodes
                    .iter()
                    .map(|(element_id, _, _)| element_id)
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );
    let evaluation = runtime
        .evaluate(
            &script,
            &projected,
            "fixture://foreign-content-breakout.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("fragment projection and XHR parser must reprocess breakout tags");
    assert_eq!(evaluation.value, serde_json::json!([expected, expected]));
    projected
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must reprocess breakout tags");
    assert_eq!(summarize_native(&projected), expected);
}

#[test]
fn foreign_fragment_breakout_keeps_target_and_uses_html_child_namespace() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("foreign-fragment-breakout")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
        "<main><svg id='context'></svg></main>",
        &NativeEngineLimits::default(),
    )
    .expect("foreign fragment host document must parse");
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const target = document.querySelector("#context");
                    target.innerHTML = "<div id='fragment-div'/><span id='fragment-child'>child</span></div>";
                    const element = target.querySelector("#fragment-div");
                    const child = target.querySelector("#fragment-child");
                    return [
                        element.namespaceURI,
                        element.parentElement.getAttribute("id"),
                        child.namespaceURI,
                        child.parentElement.getAttribute("id"),
                    ];
                })()"##,
                &document,
                "fixture://foreign-fragment-context.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("same-turn foreign fragment must reprocess breakout as HTML");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            HTML_NAMESPACE_URI,
            "context",
            HTML_NAMESPACE_URI,
            "fragment-div"
        ])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must preserve the foreign context target");
    let target = document.find_element_by_id("context").unwrap();
    let element = document.find_element_by_id("fragment-div").unwrap();
    let child = document.find_element_by_id("fragment-child").unwrap();
    assert_eq!(
        document.node(element).unwrap().namespace_uri(),
        Some(HTML_NAMESPACE_URI)
    );
    assert_eq!(
        document.node(element).and_then(NativeNode::parent),
        Some(target)
    );
    assert_eq!(
        document.node(child).unwrap().namespace_uri(),
        Some(HTML_NAMESPACE_URI)
    );
    assert_eq!(
        document.node(child).and_then(NativeNode::parent),
        Some(element)
    );
}

#[test]
fn foreign_content_breakout_end_tags_reprocess_as_html_across_parser_routes() {
    let markup = concat!(
        "<section id='zone'>",
        "<p id='open-p'>before<svg id='open-p-svg'><g id='open-p-g'></p>",
        "<span id='after-open-p'></span></g></svg>",
        "<svg id='missing-p-svg'><g id='missing-p-g'></p>",
        "<span id='after-missing-p'></span></g></svg>",
        "<svg id='br-svg'><g id='br-g'></br><span id='after-br'></span></g></svg>",
        "<math id='math-br'><mrow id='math-br-row'></br>",
        "<span id='after-math-br'></span></mrow></math>",
        "<svg id='fo-svg'><foreignObject id='foreign-object'></p>",
        "<span id='after-foreign-object-p'></span></foreignObject></svg>",
        "<math id='mi-math'><mi id='math-mi'></p>",
        "<span id='after-math-mi-p'></span></mi></math>",
        "<math id='annotation-math'><annotation-xml id='annotation-html' ",
        "encoding='APPLICATION/XHTML+XML'></p>",
        "<span id='after-annotation-p'></span></annotation-xml></math>",
        "<p id='outer-button-p'><button id='button-scope'>",
        "<svg id='button-svg'><g id='button-g'></p>",
        "<span id='after-button-scope-p'></span></g></svg>",
        "</button><span id='after-button'></span></p>",
        "<div id='html-recovery'></br><span id='html-after-br'></span>",
        "</p><span id='html-after-p'></span></div>",
        "<table id='end-mode-table'><colgroup id='end-mode-colgroup'></p></br>",
        "<col id='end-mode-col'></colgroup><tbody id='end-mode-body'>",
        "<tr id='end-mode-row'>",
        "<td id='end-mode-cell'>cell</td></tr></tbody></table>",
        "<svg id='ordinary-svg'><g id='ordinary-g'><path id='ordinary-before'></path></g>",
        "<path id='ordinary-after'></path></svg>",
        "</section>",
    );
    let expected_nodes = [
        ("open-p-svg", SVG_NAMESPACE_URI, "open-p"),
        ("open-p-g", SVG_NAMESPACE_URI, "open-p-svg"),
        ("after-open-p", HTML_NAMESPACE_URI, "zone"),
        ("missing-p-svg", SVG_NAMESPACE_URI, "zone"),
        ("missing-p-g", SVG_NAMESPACE_URI, "missing-p-svg"),
        ("after-missing-p", HTML_NAMESPACE_URI, "zone"),
        ("br-svg", SVG_NAMESPACE_URI, "zone"),
        ("br-g", SVG_NAMESPACE_URI, "br-svg"),
        ("after-br", HTML_NAMESPACE_URI, "zone"),
        ("math-br", MATHML_NAMESPACE_URI, "zone"),
        ("math-br-row", MATHML_NAMESPACE_URI, "math-br"),
        ("after-math-br", HTML_NAMESPACE_URI, "zone"),
        ("fo-svg", SVG_NAMESPACE_URI, "zone"),
        ("foreign-object", SVG_NAMESPACE_URI, "fo-svg"),
        (
            "after-foreign-object-p",
            HTML_NAMESPACE_URI,
            "foreign-object",
        ),
        ("mi-math", MATHML_NAMESPACE_URI, "zone"),
        ("math-mi", MATHML_NAMESPACE_URI, "mi-math"),
        ("after-math-mi-p", HTML_NAMESPACE_URI, "math-mi"),
        ("annotation-math", MATHML_NAMESPACE_URI, "zone"),
        ("annotation-html", MATHML_NAMESPACE_URI, "annotation-math"),
        ("after-annotation-p", HTML_NAMESPACE_URI, "annotation-html"),
        ("outer-button-p", HTML_NAMESPACE_URI, "zone"),
        ("button-svg", SVG_NAMESPACE_URI, "button-scope"),
        ("button-g", SVG_NAMESPACE_URI, "button-svg"),
        ("after-button-scope-p", HTML_NAMESPACE_URI, "button-scope"),
        ("after-button", HTML_NAMESPACE_URI, "outer-button-p"),
        ("html-after-br", HTML_NAMESPACE_URI, "html-recovery"),
        ("html-after-p", HTML_NAMESPACE_URI, "html-recovery"),
        ("end-mode-table", HTML_NAMESPACE_URI, "zone"),
        ("end-mode-colgroup", HTML_NAMESPACE_URI, "end-mode-table"),
        ("end-mode-col", HTML_NAMESPACE_URI, "end-mode-colgroup"),
        ("end-mode-body", HTML_NAMESPACE_URI, "end-mode-table"),
        ("end-mode-row", HTML_NAMESPACE_URI, "end-mode-body"),
        ("end-mode-cell", HTML_NAMESPACE_URI, "end-mode-row"),
        ("ordinary-svg", SVG_NAMESPACE_URI, "zone"),
        ("ordinary-g", SVG_NAMESPACE_URI, "ordinary-svg"),
        ("ordinary-before", SVG_NAMESPACE_URI, "ordinary-g"),
        ("ordinary-after", SVG_NAMESPACE_URI, "ordinary-svg"),
    ];
    let expected_recovery_nodes = || {
        let mut nodes = vec![
            serde_json::json!(["br", HTML_NAMESPACE_URI, "zone", ""]),
            serde_json::json!(["br", HTML_NAMESPACE_URI, "zone", ""]),
            serde_json::json!(["p", HTML_NAMESPACE_URI, "zone", "open-p"]),
            serde_json::json!(["p", HTML_NAMESPACE_URI, "zone", ""]),
            serde_json::json!(["p", HTML_NAMESPACE_URI, "foreign-object", ""]),
            serde_json::json!(["p", HTML_NAMESPACE_URI, "math-mi", ""]),
            serde_json::json!(["p", HTML_NAMESPACE_URI, "annotation-html", ""]),
            serde_json::json!(["p", HTML_NAMESPACE_URI, "zone", "outer-button-p"]),
            serde_json::json!(["p", HTML_NAMESPACE_URI, "button-scope", ""]),
            serde_json::json!(["br", HTML_NAMESPACE_URI, "html-recovery", ""]),
            serde_json::json!(["p", HTML_NAMESPACE_URI, "html-recovery", ""]),
        ];
        nodes.sort_by_key(|value| value.to_string());
        nodes
    };
    let standards_summary = expected_nodes
        .iter()
        .map(|(element_id, namespace, parent)| {
            let parent = if *element_id == "end-mode-col" {
                ""
            } else {
                *parent
            };
            serde_json::json!([namespace, parent])
        })
        .collect::<Vec<_>>();
    let summarize_native = |tree: &NativeDocument| {
        let nodes = expected_nodes
            .iter()
            .map(|(element_id, _, _)| {
                let element = tree.find_element_by_id(element_id).unwrap_or_else(|| {
                    panic!("foreign-end-breakout fixture element {element_id} must exist")
                });
                let node = tree.node(element).expect("fixture element must resolve");
                let parent_id = node
                    .parent()
                    .and_then(|parent| tree.node(parent))
                    .and_then(|parent| parent.attribute("id"))
                    .unwrap_or("");
                serde_json::json!([node.namespace_uri().unwrap(), parent_id])
            })
            .collect::<Vec<_>>();
        let root = tree.find_element_by_id("zone").unwrap();
        let mut pending = vec![root];
        let mut recovery = Vec::new();
        while let Some(id) = pending.pop() {
            let node = tree.node(id).expect("recovery traversal node must resolve");
            if matches!(node.element_name(), Some("br" | "p")) {
                let parent_id = node
                    .parent()
                    .and_then(|parent| tree.node(parent))
                    .and_then(|parent| parent.attribute("id"))
                    .unwrap_or("");
                recovery.push(serde_json::json!([
                    node.element_name().unwrap(),
                    node.namespace_uri().unwrap(),
                    parent_id,
                    node.attribute("id").unwrap_or(""),
                ]));
            }
            pending.extend(node.children().iter().copied());
        }
        recovery.sort_by_key(|value| value.to_string());
        serde_json::json!([nodes, recovery])
    };
    let mut standards_recovery_nodes = expected_recovery_nodes();
    standards_recovery_nodes.extend([
        serde_json::json!(["br", HTML_NAMESPACE_URI, "zone", ""]),
        serde_json::json!(["p", HTML_NAMESPACE_URI, "zone", ""]),
    ]);
    standards_recovery_nodes.sort_by_key(|value| value.to_string());
    let standards_expected = serde_json::json!([standards_summary, standards_recovery_nodes]);

    let direct = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("document parser must reprocess foreign end-tag breakouts");
    assert_eq!(summarize_native(&direct), standards_expected);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("foreign-end-breakout")
        .expect("native JavaScript runtime must construct");
    let mut projected =
        NativeDocument::parse("<main id='host'></main>", &NativeEngineLimits::default())
            .expect("fragment host document must parse");
    let script = r##"(() => {
            const markup = __HTML_MARKUP__;
            const ids = __ELEMENT_IDS__;
            const host = document.querySelector("#host");
            host.innerHTML = markup;
            const response = globalThis.__glassParseHtmlDocument(
                markup,
                "https://example.test/foreign-end-breakout.html",
                "text/html",
            );
            const summarize = (tree) => {
                const nodes = ids.map((id) => {
                    const element = tree.querySelector(`#${id}`);
                    return [element.namespaceURI, element.parentElement.getAttribute("id") || ""];
                });
                const recovery = ["br", "p"].flatMap((name) =>
                    Array.from(tree.querySelectorAll(name)).map((element) => [
                        element.localName,
                        element.namespaceURI,
                        element.parentElement.getAttribute("id") || "",
                        element.getAttribute("id") || "",
                    ])).sort((left, right) => JSON.stringify(left).localeCompare(JSON.stringify(right)));
                return [nodes, recovery];
            };
            return [summarize(host), summarize(response)];
        })()"##
            .replace("__HTML_MARKUP__", &serde_json::to_string(markup).unwrap())
            .replace(
                "__ELEMENT_IDS__",
                &serde_json::to_string(
                    &expected_nodes
                        .iter()
                        .map(|(element_id, _, _)| element_id)
                        .collect::<Vec<_>>(),
                )
                .unwrap(),
            );
    let evaluation = runtime
        .evaluate(
            &script,
            &projected,
            "fixture://foreign-end-breakout.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("same-turn and XHR parsers must reprocess foreign end tags");
    assert_eq!(
        evaluation.value,
        serde_json::json!([standards_expected, standards_expected])
    );
    projected
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must reprocess foreign end tags");
    assert_eq!(summarize_native(&projected), standards_expected);
}

#[test]
fn foreign_content_ordinary_end_tags_follow_stack_and_html_boundaries_across_routes() {
    let markup = concat!(
        "<section id='zone'>",
        "<svg id='matched-svg'><probe id='matched-probe'><g id='matched-g'>",
        "<path id='matched-path'></PROBE><circle id='after-foreign-match'></circle></svg>",
        "<svg id='unmatched-svg'><g id='unmatched-g'></absent>",
        "<path id='inside-unmatched'></path></g><path id='after-unmatched'></path></svg>",
        "<svg id='boundary-svg'><probe id='foreign-probe'>",
        "<foreignObject id='boundary-fo'><div id='html-boundary'>",
        "<svg id='inner-svg'><g id='inner-g'></probe>",
        "<span id='after-html-boundary'></span></div></foreignObject></probe></svg>",
        "<svg id='integration-svg'><foreignObject id='integration-point'></foreignObject>",
        "<circle id='after-integration-close'></circle></svg>",
        "</section>",
    );
    let expected_nodes = [
        ("matched-probe", SVG_NAMESPACE_URI, "matched-svg"),
        ("matched-path", SVG_NAMESPACE_URI, "matched-g"),
        ("after-foreign-match", SVG_NAMESPACE_URI, "matched-svg"),
        ("inside-unmatched", SVG_NAMESPACE_URI, "unmatched-g"),
        ("after-unmatched", SVG_NAMESPACE_URI, "unmatched-svg"),
        ("foreign-probe", SVG_NAMESPACE_URI, "boundary-svg"),
        ("html-boundary", HTML_NAMESPACE_URI, "boundary-fo"),
        ("inner-svg", SVG_NAMESPACE_URI, "html-boundary"),
        ("inner-g", SVG_NAMESPACE_URI, "inner-svg"),
        ("after-html-boundary", HTML_NAMESPACE_URI, "html-boundary"),
        ("integration-point", SVG_NAMESPACE_URI, "integration-svg"),
        (
            "after-integration-close",
            SVG_NAMESPACE_URI,
            "integration-svg",
        ),
    ];
    let expected = serde_json::Value::Array(
        expected_nodes
            .iter()
            .map(|(_, namespace, parent)| serde_json::json!([namespace, parent]))
            .collect(),
    );
    let summarize_native = |tree: &NativeDocument| {
        serde_json::Value::Array(
            expected_nodes
                .iter()
                .map(|(element_id, _, _)| {
                    let element = tree.find_element_by_id(element_id).unwrap_or_else(|| {
                        panic!("foreign end-tag fixture element {element_id} must exist")
                    });
                    let node = tree.node(element).expect("fixture element must resolve");
                    let parent_id = node
                        .parent()
                        .and_then(|parent| tree.node(parent))
                        .and_then(|parent| parent.attribute("id"))
                        .expect("fixture element parent must have an id");
                    serde_json::json!([node.namespace_uri().unwrap(), parent_id])
                })
                .collect(),
        )
    };

    let direct = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("document parser must dispatch ordinary foreign end tags");
    assert_eq!(summarize_native(&direct), expected);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("foreign-end-dispatch")
        .expect("native JavaScript runtime must construct");
    let mut projected =
        NativeDocument::parse("<main id='host'></main>", &NativeEngineLimits::default())
            .expect("fragment host document must parse");
    let script = r##"(() => {
            const markup = __HTML_MARKUP__;
            const ids = __ELEMENT_IDS__;
            const host = document.querySelector("#host");
            host.innerHTML = markup;
            const response = globalThis.__glassParseHtmlDocument(
                markup,
                "https://example.test/foreign-end-dispatch.html",
                "text/html",
            );
            const summarize = (tree) => ids.map((id) => {
                const element = tree.querySelector(`#${id}`);
                return [element.namespaceURI, element.parentElement.getAttribute("id")];
            });
            return [summarize(host), summarize(response)];
        })()"##
        .replace("__HTML_MARKUP__", &serde_json::to_string(markup).unwrap())
        .replace(
            "__ELEMENT_IDS__",
            &serde_json::to_string(
                &expected_nodes
                    .iter()
                    .map(|(element_id, _, _)| element_id)
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );
    let evaluation = runtime
        .evaluate(
            &script,
            &projected,
            "fixture://foreign-end-dispatch.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("same-turn and XHR routes must share foreign end-tag dispatch");
    assert_eq!(evaluation.value, serde_json::json!([expected, expected]));
    projected
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must share foreign end-tag dispatch");
    assert_eq!(summarize_native(&projected), expected);
}

#[test]
fn foreign_parser_attributes_adjust_names_and_namespaces_across_routes() {
    const EXPECTED_SVG_ADJUSTMENTS: &[(&str, &str)] = &[
        ("attributename", "attributeName"),
        ("attributetype", "attributeType"),
        ("basefrequency", "baseFrequency"),
        ("baseprofile", "baseProfile"),
        ("calcmode", "calcMode"),
        ("clippathunits", "clipPathUnits"),
        ("diffuseconstant", "diffuseConstant"),
        ("edgemode", "edgeMode"),
        ("filterunits", "filterUnits"),
        ("glyphref", "glyphRef"),
        ("gradienttransform", "gradientTransform"),
        ("gradientunits", "gradientUnits"),
        ("kernelmatrix", "kernelMatrix"),
        ("kernelunitlength", "kernelUnitLength"),
        ("keypoints", "keyPoints"),
        ("keysplines", "keySplines"),
        ("keytimes", "keyTimes"),
        ("lengthadjust", "lengthAdjust"),
        ("limitingconeangle", "limitingConeAngle"),
        ("markerheight", "markerHeight"),
        ("markerunits", "markerUnits"),
        ("markerwidth", "markerWidth"),
        ("maskcontentunits", "maskContentUnits"),
        ("maskunits", "maskUnits"),
        ("numoctaves", "numOctaves"),
        ("pathlength", "pathLength"),
        ("patterncontentunits", "patternContentUnits"),
        ("patterntransform", "patternTransform"),
        ("patternunits", "patternUnits"),
        ("pointsatx", "pointsAtX"),
        ("pointsaty", "pointsAtY"),
        ("pointsatz", "pointsAtZ"),
        ("preservealpha", "preserveAlpha"),
        ("preserveaspectratio", "preserveAspectRatio"),
        ("primitiveunits", "primitiveUnits"),
        ("refx", "refX"),
        ("refy", "refY"),
        ("repeatcount", "repeatCount"),
        ("repeatdur", "repeatDur"),
        ("requiredextensions", "requiredExtensions"),
        ("requiredfeatures", "requiredFeatures"),
        ("specularconstant", "specularConstant"),
        ("specularexponent", "specularExponent"),
        ("spreadmethod", "spreadMethod"),
        ("startoffset", "startOffset"),
        ("stddeviation", "stdDeviation"),
        ("stitchtiles", "stitchTiles"),
        ("surfacescale", "surfaceScale"),
        ("systemlanguage", "systemLanguage"),
        ("tablevalues", "tableValues"),
        ("targetx", "targetX"),
        ("targety", "targetY"),
        ("textlength", "textLength"),
        ("viewbox", "viewBox"),
        ("viewtarget", "viewTarget"),
        ("xchannelselector", "xChannelSelector"),
        ("ychannelselector", "yChannelSelector"),
        ("zoomandpan", "zoomAndPan"),
    ];
    const FOREIGN_ATTRIBUTES: &[(&str, Option<&str>, &str, &str)] = &[
        (
            "xlink:actuate",
            Some("xlink"),
            "actuate",
            XLINK_NAMESPACE_URI,
        ),
        (
            "xlink:arcrole",
            Some("xlink"),
            "arcrole",
            XLINK_NAMESPACE_URI,
        ),
        ("xlink:href", Some("xlink"), "href", XLINK_NAMESPACE_URI),
        ("xlink:role", Some("xlink"), "role", XLINK_NAMESPACE_URI),
        ("xlink:show", Some("xlink"), "show", XLINK_NAMESPACE_URI),
        ("xlink:title", Some("xlink"), "title", XLINK_NAMESPACE_URI),
        ("xlink:type", Some("xlink"), "type", XLINK_NAMESPACE_URI),
        ("xml:lang", Some("xml"), "lang", XML_NAMESPACE_URI),
        ("xml:space", Some("xml"), "space", XML_NAMESPACE_URI),
        ("xmlns", None, "xmlns", XMLNS_NAMESPACE_URI),
        ("xmlns:xlink", Some("xmlns"), "xlink", XMLNS_NAMESPACE_URI),
    ];
    const HTML_PREFIX_ATTRIBUTES: &[(&str, &str)] = &[
        ("xlink:href", "plain-href"),
        ("xml:lang", "plain-lang"),
        ("xmlns", "plain-xmlns"),
        ("xmlns:xlink", "plain-xmlns-xlink"),
    ];

    assert_eq!(
        SVG_ATTRIBUTE_NAME_ADJUSTMENTS, EXPECTED_SVG_ADJUSTMENTS,
        "the native SVG map must match the complete pinned WHATWG table"
    );

    let mut svg_attributes = EXPECTED_SVG_ADJUSTMENTS
        .iter()
        .filter(|(token_name, _)| *token_name != "viewbox")
        .map(|(token_name, _)| format!("{token_name}='value-{token_name}'"))
        .collect::<Vec<_>>()
        .join(" ");
    svg_attributes.push_str(" viewbox='first-viewbox' VIEWBOX='second-viewbox'");
    for (name, _, _, _) in FOREIGN_ATTRIBUTES {
        svg_attributes.push_str(&format!(" {name}='value-{name}'"));
    }
    let html_attributes = HTML_PREFIX_ATTRIBUTES
        .iter()
        .map(|(name, value)| format!("{name}='{value}'"))
        .collect::<Vec<_>>()
        .join(" ");
    let markup = format!(
        "<section id='zone'><svg id='svg' {svg_attributes}></svg>\
             <math id='math' definitionurl='value-definitionurl'>\
             <mi id='mi' definitionurl='value-mi-definitionurl' xlink:href='math-href'></mi>\
             </math><div id='plain' {html_attributes}></div></section>"
    );
    let verify_native = |tree: &NativeDocument| {
        let svg = tree
            .node(
                tree.find_element_by_id("svg")
                    .expect("SVG fixture must exist"),
            )
            .expect("SVG fixture node must resolve");
        for (token_name, adjusted_name) in EXPECTED_SVG_ADJUSTMENTS {
            let expected_value = if *token_name == "viewbox" {
                "first-viewbox".to_owned()
            } else {
                format!("value-{token_name}")
            };
            assert_eq!(
                svg.attribute_with_namespace_uri(adjusted_name),
                Some((expected_value.as_str(), None)),
                "adjusted SVG attribute {adjusted_name} must be unnamespaced"
            );
            assert!(
                !svg.attributes().unwrap().contains_key(*token_name),
                "raw SVG token name {token_name} must not survive adjustment"
            );
        }
        for (name, _, _, namespace_uri) in FOREIGN_ATTRIBUTES {
            let expected_value = format!("value-{name}");
            assert_eq!(
                svg.attribute_with_namespace_uri(name),
                Some((expected_value.as_str(), Some(*namespace_uri))),
                "foreign attribute {name} must preserve its namespace"
            );
        }
        assert_eq!(
            svg.attribute_in_namespace(Some(XLINK_NAMESPACE_URI), "href"),
            Some("value-xlink:href")
        );
        let math = tree
            .node(
                tree.find_element_by_id("math")
                    .expect("MathML fixture must exist"),
            )
            .expect("MathML fixture node must resolve");
        assert_eq!(
            math.attribute_with_namespace_uri("definitionURL"),
            Some(("value-definitionurl", None))
        );
        let mi = tree
            .node(
                tree.find_element_by_id("mi")
                    .expect("MathML child must exist"),
            )
            .expect("MathML child node must resolve");
        assert_eq!(
            mi.attribute_with_namespace_uri("definitionURL"),
            Some(("value-mi-definitionurl", None))
        );
        assert_eq!(
            mi.attribute_with_namespace_uri("xlink:href"),
            Some(("math-href", Some(XLINK_NAMESPACE_URI)))
        );
        let plain = tree
            .node(
                tree.find_element_by_id("plain")
                    .expect("HTML fixture must exist"),
            )
            .expect("HTML fixture node must resolve");
        for (name, value) in HTML_PREFIX_ATTRIBUTES {
            assert_eq!(
                plain.attribute_with_namespace_uri(name),
                Some((*value, None)),
                "HTML attribute {name} must remain unnamespaced"
            );
        }
        assert_eq!(
            plain.attribute_in_namespace(None, "xlink:href"),
            Some("plain-href")
        );
        assert_eq!(plain.attribute_in_namespace(None, "href"), None);
        assert_eq!(plain.attribute_in_any_namespace_with_uri("href"), None);
        let zone = tree
            .find_element_by_id("zone")
            .expect("fixture root must exist");
        let serialized = tree.element_inner_html(zone, 256 * 1024);
        for (_, adjusted_name) in EXPECTED_SVG_ADJUSTMENTS {
            assert!(
                serialized.contains(&format!(" {adjusted_name}=\"")),
                "HTML serialization must retain adjusted name {adjusted_name}"
            );
        }
        assert!(serialized.contains(" viewBox=\"first-viewbox\""));
        assert!(!serialized.contains(" viewbox=\""));
        for (name, _, _, _) in FOREIGN_ATTRIBUTES {
            assert!(serialized.contains(&format!(" {name}=\"")));
        }
        assert!(serialized.contains(" definitionURL=\"value-definitionurl\""));
    };

    let direct = NativeDocument::parse(&markup, &NativeEngineLimits::default())
        .expect("direct HTML document parsing must adjust foreign attributes");
    verify_native(&direct);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("foreign-attribute-adjustment")
        .expect("native JavaScript runtime must construct");
    let mut projected =
        NativeDocument::parse("<main id='host'></main>", &NativeEngineLimits::default())
            .expect("fragment host document must parse");
    let script = r##"(() => {
            const markup = __HTML_MARKUP__;
            const svgAdjustments = __SVG_ADJUSTMENTS__;
            const foreignAttributes = __FOREIGN_ATTRIBUTES__;
            const htmlAttributes = __HTML_ATTRIBUTES__;
            const xlinkNamespace = "http://www.w3.org/1999/xlink";
            const xmlNamespace = "http://www.w3.org/XML/1998/namespace";
            const xmlnsNamespace = "http://www.w3.org/2000/xmlns/";
            const check = (condition, message) => {
                if (!condition) throw new Error("foreign-attribute fixture: " + message);
            };
            const getAttrNode = (element, name) => typeof element.getAttributeNode === "function"
                ? element.getAttributeNode(name)
                : element.attributes.getNamedItem(name);
            const getAttrNodeNS = (element, namespaceURI, localName) =>
                typeof element.getAttributeNodeNS === "function"
                    ? element.getAttributeNodeNS(namespaceURI, localName)
                    : element.attributes.getNamedItemNS(namespaceURI, localName);
            const verifyTree = (tree) => {
                const svg = tree.querySelector("#svg");
                const math = tree.querySelector("#math");
                const mi = tree.querySelector("#mi");
                const plain = tree.querySelector("#plain");
                check(svg && math && mi && plain, "all fixture elements must exist");
                for (const [tokenName, adjustedName] of svgAdjustments) {
                    const value = tokenName === "viewbox"
                        ? "first-viewbox"
                        : "value-" + tokenName;
                    const attribute = getAttrNode(svg, adjustedName);
                    check(attribute !== null, "SVG Attr " + adjustedName + " must exist");
                    check(attribute.name === adjustedName, "SVG qualified name " + adjustedName);
                    check(attribute.prefix === null, "SVG prefix for " + adjustedName);
                    check(attribute.localName === adjustedName, "SVG local name " + adjustedName);
                    check(attribute.namespaceURI === null, "SVG namespace for " + adjustedName);
                    check(attribute.value === value, "SVG value for " + adjustedName);
                    check(svg.getAttribute(adjustedName) === value, "SVG getAttribute " + adjustedName);
                    check(svg.getAttribute(tokenName) === null, "SVG source case " + tokenName);
                    check(svg.getAttributeNS(null, adjustedName) === value, "SVG getAttributeNS " + adjustedName);
                }
                for (const [qualifiedName, prefix, localName, namespaceURI] of foreignAttributes) {
                    const value = "value-" + qualifiedName;
                    const attribute = getAttrNode(svg, qualifiedName);
                    const namespaced = getAttrNodeNS(svg, namespaceURI, localName);
                    check(attribute !== null && namespaced === attribute, "Attr identity " + qualifiedName);
                    check(attribute.name === qualifiedName, "qualified name " + qualifiedName);
                    check(attribute.prefix === prefix, "prefix " + qualifiedName);
                    check(attribute.localName === localName, "local name " + qualifiedName);
                    check(attribute.namespaceURI === namespaceURI, "namespace " + qualifiedName);
                    check(attribute.value === value, "value " + qualifiedName);
                    check(svg.getAttribute(qualifiedName) === value, "getAttribute " + qualifiedName);
                    check(svg.getAttributeNS(namespaceURI, localName) === value, "getAttributeNS " + qualifiedName);
                    check(svg.hasAttributeNS(namespaceURI, localName), "hasAttributeNS " + qualifiedName);
                }
                for (const [name, value] of htmlAttributes) {
                    const attribute = getAttrNode(plain, name);
                    check(attribute !== null, "HTML Attr " + name + " must exist");
                    check(attribute.name === name, "HTML qualified name " + name);
                    check(attribute.prefix === null, "HTML prefix " + name + ": " + JSON.stringify([attribute.prefix, attribute.namespaceURI]));
                    check(attribute.localName === name, "HTML local name " + name);
                    check(attribute.namespaceURI === null, "HTML namespace " + name);
                    check(attribute.value === value, "HTML value " + name);
                    check(plain.getAttributeNS(null, name) === value, "HTML null namespace lookup " + name);
                    check(!plain.hasAttributeNS(xlinkNamespace, "href"), "HTML XLink must remain unnamespaced");
                    check(!plain.hasAttributeNS(xmlNamespace, "lang"), "HTML XML must remain unnamespaced");
                    check(!plain.hasAttributeNS(xmlnsNamespace, name === "xmlns:xlink" ? "xlink" : "xmlns"),
                        "HTML XMLNS must remain unnamespaced");
                }
                const definition = getAttrNode(math, "definitionURL");
                check(definition && definition.name === "definitionURL"
                    && definition.localName === "definitionURL"
                    && definition.namespaceURI === null
                    && definition.value === "value-definitionurl", "MathML definitionURL");
                check(math.getAttribute("definitionurl") === null, "MathML source case");
                const childDefinition = getAttrNode(mi, "definitionURL");
                check(childDefinition && childDefinition.localName === "definitionURL"
                    && childDefinition.value === "value-mi-definitionurl", "nested MathML definitionURL");
                const mathHref = mi.getAttributeNodeNS(xlinkNamespace, "href");
                check(mathHref && mathHref.prefix === "xlink"
                    && mathHref.localName === "href"
                    && mathHref.namespaceURI === xlinkNamespace
                    && mathHref.value === "math-href", "MathML XLink adjustment");
                const serialized = svg.outerHTML;
                for (const [, adjustedName] of svgAdjustments) {
                    check(serialized.includes(" " + adjustedName + "="), "SVG serialization " + adjustedName);
                }
                check(serialized.includes(" viewBox=") && !serialized.includes(" viewbox="),
                    "first duplicate wins and serializes the canonical name");
                for (const [qualifiedName] of foreignAttributes) {
                    check(serialized.includes(" " + qualifiedName + "="), "foreign serialization " + qualifiedName);
                }
                return true;
            };
            const host = document.querySelector("#host");
            host.innerHTML = markup;
            const xhrDocument = globalThis.__glassParseHtmlDocument(
                markup,
                "https://example.test/foreign-attribute-adjustment.html",
                "text/html",
            );
            const clone = host.querySelector("#svg").cloneNode(true);
            const cloneHref = clone.getAttributeNodeNS(xlinkNamespace, "href");
            check(cloneHref && cloneHref.name === "xlink:href"
                && cloneHref.prefix === "xlink" && cloneHref.localName === "href"
                && cloneHref.namespaceURI === xlinkNamespace, "clone preserves XLink identity");
            clone.removeAttributeNS(xlinkNamespace, "href");
            check(clone.getAttributeNS(xlinkNamespace, "href") === null
                && clone.getAttribute("xlink:href") === null, "namespaced removal uses local name");
            check(host.querySelector("#svg").getAttributeNS(xlinkNamespace, "href") === "value-xlink:href",
                "clone removal leaves the source intact");
            return [verifyTree(host), verifyTree(xhrDocument), true];
        })()"##
            .replace("__HTML_MARKUP__", &serde_json::to_string(&markup).unwrap())
            .replace(
                "__SVG_ADJUSTMENTS__",
                &serde_json::to_string(EXPECTED_SVG_ADJUSTMENTS).unwrap(),
            )
            .replace(
                "__FOREIGN_ATTRIBUTES__",
                &serde_json::to_string(FOREIGN_ATTRIBUTES).unwrap(),
            )
            .replace(
                "__HTML_ATTRIBUTES__",
                &serde_json::to_string(HTML_PREFIX_ATTRIBUTES).unwrap(),
            );
    let evaluation = runtime
        .evaluate(
            &script,
            &projected,
            "fixture://foreign-attribute-adjustment.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("fragment projection and XHR parsing must preserve adjusted attributes");
    assert_eq!(evaluation.value, serde_json::json!([true, true, true]));
    projected
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must preserve adjusted foreign attributes");
    verify_native(&projected);
}

#[test]
fn foreign_fragment_ordinary_end_tags_preserve_target_and_namespace() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("foreign-fragment-end-tags")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
        "<main><svg id='context'></svg></main>",
        &NativeEngineLimits::default(),
    )
    .expect("foreign fragment host document must parse");
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const target = document.querySelector("#context");
                    target.innerHTML = "</svg><g id='fragment-child'><path id='fragment-grandchild'></path></g></absent>";
                    const child = target.querySelector("#fragment-child");
                    const grandchild = target.querySelector("#fragment-grandchild");
                    return [
                        target.namespaceURI,
                        child.namespaceURI,
                        child.parentElement === target,
                        grandchild.namespaceURI,
                        grandchild.parentElement === child,
                    ];
                })()"##,
                &document,
                "fixture://foreign-fragment-end-tags.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("foreign fragment root end tags must not pop or recontextualize the target");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            SVG_NAMESPACE_URI,
            SVG_NAMESPACE_URI,
            true,
            SVG_NAMESPACE_URI,
            true
        ])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must preserve the foreign context element");
    let target = document.find_element_by_id("context").unwrap();
    let child = document.find_element_by_id("fragment-child").unwrap();
    let grandchild = document.find_element_by_id("fragment-grandchild").unwrap();
    assert_eq!(
        document.node(child).unwrap().namespace_uri(),
        Some(SVG_NAMESPACE_URI)
    );
    assert_eq!(
        document.node(child).and_then(NativeNode::parent),
        Some(target)
    );
    assert_eq!(
        document.node(grandchild).unwrap().namespace_uri(),
        Some(SVG_NAMESPACE_URI)
    );
    assert_eq!(
        document.node(grandchild).and_then(NativeNode::parent),
        Some(child)
    );
}

#[test]
fn foreign_fragment_end_tag_breakout_preserves_target_and_html_rules() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("foreign-fragment-end-breakout")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
            "<main><svg id='context'></svg><table><colgroup id='html-context'></colgroup></table></main>",
            &NativeEngineLimits::default(),
        )
        .expect("foreign fragment host document must parse");
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const target = document.querySelector("#context");
                    target.innerHTML = "</br><span id='after-br'></span></p><span id='after-p'></span>";
                    const br = target.querySelector("br");
                    const afterBr = target.querySelector("#after-br");
                    const paragraph = target.querySelector("p");
                    const afterP = target.querySelector("#after-p");
                    return [
                        [br.namespaceURI, br.parentElement.getAttribute("id")],
                        [afterBr.namespaceURI, afterBr.parentElement.getAttribute("id")],
                        [paragraph.namespaceURI, paragraph.parentElement.getAttribute("id")],
                        [afterP.namespaceURI, afterP.parentElement.getAttribute("id")],
                    ];
                })()"##,
                &document,
                "fixture://foreign-fragment-end-breakout.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("foreign fragment end tags must use HTML insertion rules");
    let expected = serde_json::json!([
        [HTML_NAMESPACE_URI, "context"],
        [HTML_NAMESPACE_URI, "context"],
        [HTML_NAMESPACE_URI, "context"],
        [HTML_NAMESPACE_URI, "context"],
    ]);
    assert_eq!(evaluation.value, expected);
    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must preserve foreign target and HTML children");
    let target = document.find_element_by_id("context").unwrap();
    for id in ["after-br", "after-p"] {
        let child = document.find_element_by_id(id).unwrap();
        assert_eq!(
            document.node(child).unwrap().namespace_uri(),
            Some(HTML_NAMESPACE_URI)
        );
        assert_eq!(
            document.node(child).and_then(NativeNode::parent),
            Some(target)
        );
    }
    let br = document.element_inner_html(target, 4096);
    assert!(br.contains("<br>"));
    assert!(br.contains("<p></p>"));

    let column_group_evaluation = runtime
        .evaluate(
            r##"(() => {
                    const context = document.querySelector("#html-context");
                    context.innerHTML = "</p></br><col id='column'>";
                    const column = context.querySelector("#column");
                    return [
                        context.querySelectorAll("p").length,
                        context.querySelectorAll("br").length,
                        column.namespaceURI,
                        column.parentElement === context,
                    ];
                })()"##,
            &document,
            "fixture://column-group-end-tags.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("column-group fragment must retain its HTML insertion mode");
    assert_eq!(
        column_group_evaluation.value,
        serde_json::json!([0, 0, HTML_NAMESPACE_URI, true])
    );
    document
        .apply_script_commands(&column_group_evaluation.commands)
        .expect("column-group fragment commit must preserve ignored end tags");
    let context = document.find_element_by_id("html-context").unwrap();
    let column = document.find_element_by_id("column").unwrap();
    assert_eq!(
        document.node(column).unwrap().namespace_uri(),
        Some(HTML_NAMESPACE_URI)
    );
    assert_eq!(
        document.node(column).and_then(NativeNode::parent),
        Some(context)
    );
}

#[test]
fn html_void_elements_respect_namespace_across_parser_routes() {
    let markup = concat!(
        "<section id='zone'><svg id='svg-root'>",
        "<area id='svg-area'><g id='svg-area-child'>svg child</g></area>",
        "<area id='svg-self-closing'/><g id='svg-after-self-closing'></g>",
        "</svg><math id='math-root'>",
        "<area id='math-area'><mi id='math-area-child'>math child</mi></area>",
        "<area id='math-self-closing'/><mo id='math-after-self-closing'></mo>",
        "</math><area id='html-area'><span id='html-after-area'>html sibling</span>",
        "</section>",
    );
    let expected_nodes = [
        ("svg-root", SVG_NAMESPACE_URI, "zone"),
        ("svg-area", SVG_NAMESPACE_URI, "svg-root"),
        ("svg-area-child", SVG_NAMESPACE_URI, "svg-area"),
        ("svg-self-closing", SVG_NAMESPACE_URI, "svg-root"),
        ("svg-after-self-closing", SVG_NAMESPACE_URI, "svg-root"),
        ("math-root", MATHML_NAMESPACE_URI, "zone"),
        ("math-area", MATHML_NAMESPACE_URI, "math-root"),
        ("math-area-child", MATHML_NAMESPACE_URI, "math-area"),
        ("math-self-closing", MATHML_NAMESPACE_URI, "math-root"),
        ("math-after-self-closing", MATHML_NAMESPACE_URI, "math-root"),
        ("html-area", HTML_NAMESPACE_URI, "zone"),
        ("html-after-area", HTML_NAMESPACE_URI, "zone"),
    ];
    let expected = expected_nodes
        .iter()
        .map(|(_, namespace, parent)| serde_json::json!([namespace, parent]))
        .collect::<Vec<_>>();
    let summarize_native = |tree: &NativeDocument| {
        expected_nodes
            .iter()
            .map(|(element_id, _, _)| {
                let element = tree.find_element_by_id(element_id).unwrap_or_else(|| {
                    panic!("void-element fixture element {element_id} must exist")
                });
                let node = tree
                    .node(element)
                    .expect("void-element fixture element must resolve");
                let parent_id = node
                    .parent()
                    .and_then(|parent| tree.node(parent))
                    .and_then(|parent| parent.attribute("id"))
                    .expect("void-element fixture parent must have an id");
                serde_json::json!([node.namespace_uri().unwrap(), parent_id])
            })
            .collect::<Vec<_>>()
    };

    let direct = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("document parser must apply void behavior by namespace");
    assert_eq!(summarize_native(&direct), expected);
    let serialized_zone =
        direct.element_inner_html(direct.find_element_by_id("zone").unwrap(), 4096);
    assert!(
        serialized_zone
            .contains("<area id=\"svg-area\"><g id=\"svg-area-child\">svg child</g></area>")
    );
    assert!(serialized_zone.contains("<area id=\"svg-self-closing\"></area>"));
    assert!(
        serialized_zone
            .contains("<area id=\"html-area\"><span id=\"html-after-area\">html sibling</span>")
    );
    let svg_area = direct.find_element_by_id("svg-area").unwrap();
    assert_eq!(
        direct.element_inner_html(svg_area, 1024),
        "<g id=\"svg-area-child\">svg child</g>"
    );

    let runtime = NativeJavaScriptRuntime::new_with_context_id("html-void-elements-context")
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
                "https://example.test/html-void-elements.html",
                "text/html",
            );
            const ids = __ELEMENT_IDS__;
            const summarize = (tree) => ids.map((id) => {
                const element = tree.querySelector(`#${id}`);
                return [element.namespaceURI, element.parentElement.getAttribute("id")];
            });
            const foreignArea = response.querySelector("#svg-area");
            const htmlArea = response.querySelector("#html-area");
            return [
                summarize(root),
                summarize(response),
                globalThis.__glassSerializeXmlNode(foreignArea),
                globalThis.__glassSerializeXmlNode(htmlArea),
            ];
        })()"##
        .replace("__HTML_MARKUP__", &serde_json::to_string(markup).unwrap())
        .replace(
            "__ELEMENT_IDS__",
            &serde_json::to_string(
                &expected_nodes
                    .iter()
                    .map(|(element_id, _, _)| element_id)
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://html-void-elements.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("fragment projection and XHR parser must respect foreign void names");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            expected,
            expected,
            "<area id=\"svg-area\"><g id=\"svg-area-child\">svg child</g></area>",
            "<area id=\"html-area\">"
        ])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must respect foreign void names");
    assert_eq!(summarize_native(&document), expected);
    let svg_area = document.find_element_by_id("svg-area").unwrap();
    assert_eq!(
        document.element_inner_html(svg_area, 1024),
        "<g id=\"svg-area-child\">svg child</g>"
    );

    let void_target = runtime
        .evaluate(
            r##"(() => {
                    const target = document.querySelector("#html-area");
                    target.innerHTML = "<span id='void-target-child'>child</span>";
                    const child = target.querySelector("#void-target-child");
                    return [
                        target.namespaceURI,
                        child.namespaceURI,
                        child.parentElement.getAttribute("id"),
                        target.innerHTML,
                    ];
                })()"##,
            &document,
            "fixture://html-void-elements.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("innerHTML on an HTML void-named target must parse its fragment");
    assert_eq!(
        void_target.value,
        serde_json::json!([
            HTML_NAMESPACE_URI,
            HTML_NAMESPACE_URI,
            "html-area",
            "<span id=\"void-target-child\">child</span>"
        ])
    );
    document
        .apply_script_commands(&void_target.commands)
        .expect("Rust must commit fragment children for an HTML void-named target");
    let target = document.find_element_by_id("html-area").unwrap();
    let child = document.find_element_by_id("void-target-child").unwrap();
    assert_eq!(
        document.node(child).and_then(NativeNode::parent),
        Some(target)
    );
    assert_eq!(
        document.element_inner_html(target, 1024),
        "<span id=\"void-target-child\">child</span>"
    );
}

#[test]
fn implied_table_containers_follow_html_namespace_transitions_only() {
    let document = NativeDocument::parse(
            "<svg><table id='svg-table'><tr id='svg-row'><td>svg</td></tr></table><foreignObject><table id='html-table'><tr id='html-row'><td>html</td></tr></table></foreignObject></svg>",
            &NativeEngineLimits::default(),
        )
        .unwrap();

    for (table_id, row_id) in [("svg-table", "svg-row"), ("html-table", "html-row")] {
        let table = document.find_element_by_id(table_id).unwrap();
        let row = document.find_element_by_id(row_id).unwrap();
        assert_eq!(
            document.node(table).and_then(NativeNode::namespace_uri),
            Some(HTML_NAMESPACE_URI)
        );
        let implied_body = document.node(table).unwrap().children()[0];
        assert_eq!(
            document
                .node(implied_body)
                .and_then(NativeNode::element_name),
            Some("tbody")
        );
        assert_eq!(document.node(implied_body).unwrap().children(), &[row]);
        assert_eq!(
            document.node(row).and_then(NativeNode::namespace_uri),
            Some(HTML_NAMESPACE_URI)
        );
    }
}

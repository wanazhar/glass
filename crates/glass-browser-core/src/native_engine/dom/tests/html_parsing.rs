use super::*;

#[test]
fn content_wire_accepts_large_data_attributes_and_rejects_oversized_values() {
    let limits = NativeEngineLimits::default();
    let value = "x".repeat(64 * 1024);
    let source = format!("<div id='payload' data-state='{value}'></div>");
    let document = NativeDocument::parse(&source, &limits).unwrap();
    let payload = document.find_element_by_id("payload").unwrap();
    assert_eq!(
        document.node(payload).unwrap().attribute("data-state"),
        Some(value.as_str())
    );

    let value = "x".repeat(64 * 1024 + 1);
    let source = format!("<div id='payload' data-state='{value}'></div>");
    let document = NativeDocument::parse(&source, &limits).unwrap();
    assert!(
        NativeDocument::from_content_wire(
            document.to_content_wire(),
            &limits,
            document.generation()
        )
        .is_err()
    );
}

#[test]
fn parses_title_entities_and_visible_text() {
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse(
            "<!doctype html><html><head><title>Example</title><style>.x{}</style></head><body><p>Hello &amp; <span>Glass</span></p><p hidden>secret</p></body></html>",
            &limits,
        )
        .unwrap();
    assert_eq!(document.title(1024), ("Example".into(), false));
    assert_eq!(document.visible_text(1024), ("Hello & Glass".into(), false));
    assert!(document.node_count() > 1);
}

#[test]
fn author_stylesheet_can_override_browser_body_margin() {
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse_with_stylesheets_inline_policy_and_referrer_policy(
        "<style>body { margin: 0px !important; }</style><div>content</div>",
        &limits,
        &[],
        1,
        None,
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
    )
    .unwrap();
    let body = document.find_element(document.root(), "body").unwrap();
    assert_eq!(document.computed_style_for_layout(body).margin().top(), 0);
}

#[test]
fn html_parser_standards_route_preserves_basic_legacy_text_projection() {
    let source = "<!doctype html><p id='content'>before <b>after</b></p>";
    let limits = NativeEngineLimits::default();
    let standards = NativeDocument::parse(source, &limits).unwrap();
    let legacy = NativeDocument::parse_legacy_with_stylesheets_and_inline_style_policy(
        source,
        &limits,
        &[],
        1,
        None,
    )
    .unwrap();

    assert_eq!(standards.visible_text(1024), legacy.visible_text(1024));
    assert!(standards.find_element_by_id("content").is_some());
    assert!(legacy.find_element_by_id("content").is_some());
}

#[test]
fn html_parser_standards_route_maps_temporary_tree_limits_to_typed_errors() {
    let source = "<html><body>a<!--one--><!--two--></body></html>";
    let limits = NativeEngineLimits {
        max_nodes: 3,
        ..NativeEngineLimits::default()
    };
    assert!(matches!(
        NativeDocument::parse(source, &limits),
        Err(NativeEngineError::LimitExceeded {
            resource,
            limit: 6,
            actual: 7,
        }) if resource == "temporary HTML parser nodes"
    ));

    let limits = NativeEngineLimits {
        max_dom_depth: 2,
        ..NativeEngineLimits::default()
    };
    assert!(matches!(
        NativeDocument::parse("<div><span>deep</span></div>", &limits),
        Err(NativeEngineError::LimitExceeded {
            resource,
            limit: 2,
            actual: 3,
        }) if resource == "DOM depth"
    ));
}

#[test]
fn visible_text_excludes_html_iframe_fallback_descendants() {
    let document = NativeDocument::parse(
        "<main>before <iframe><strong>fallback content</strong></iframe> after</main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();

    assert_eq!(document.visible_text(1024), ("before after".into(), false));
    let frame = document.find_element(document.root(), "iframe").unwrap();
    let mut fallback_dom_text = String::new();
    document.collect_raw_text(frame, &mut fallback_dom_text);
    assert_eq!(fallback_dom_text, "<strong>fallback content</strong>");
}

#[test]
fn discovers_only_enforced_csp_meta_policies_in_head() {
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse(
            "<html><head><meta http-equiv='Content-Security-Policy' content=\"script-src 'self'\"><meta http-equiv='Content-Security-Policy-Report-Only' content=\"script-src 'none'\"></head><body><meta http-equiv='Content-Security-Policy' content=\"script-src 'none'\"></body></html>",
            &limits,
        )
        .unwrap();
    assert_eq!(
        document.content_security_policy_meta(),
        vec!["script-src 'self'".to_owned()]
    );
}

#[test]
fn csp_meta_policy_ledger_only_processes_each_node_once() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(
            "<html><head><meta http-equiv='Content-Security-Policy' content=\"script-src 'self'\"></head><body></body></html>",
            &limits,
        )
        .unwrap();

    let pending = document.unprocessed_content_security_policy_meta();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, "script-src 'self'");
    document.mark_content_security_policy_meta_processed();
    assert!(
        document
            .unprocessed_content_security_policy_meta()
            .is_empty()
    );
}

#[test]
fn preserves_comments_and_doctype_without_affecting_visible_text() {
    let document = NativeDocument::parse(
            "<!DOCTYPE html PUBLIC \"public-id\" \"system-id\"><!--before--><html><body><!--inside-->Visible</body></html>",
            &NativeEngineLimits::default(),
        )
        .unwrap();

    let root = document.root();
    let root_children = document.node(root).unwrap().children();
    assert!(matches!(
        document.node(root_children[0]).map(NativeNode::kind),
        Some(NativeNodeKind::DocumentType {
            name,
            public_id: Some(public_id),
            system_id: Some(system_id),
        }) if name == "html" && public_id == "public-id" && system_id == "system-id"
    ));
    assert!(matches!(
        document.node(root_children[1]).map(NativeNode::kind),
        Some(NativeNodeKind::Comment(value)) if value == "before"
    ));

    let snapshot = document.script_snapshot_for_viewport(1024, Viewport::default());
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.node_type == 10 && node.node_name == "html")
    );
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.node_type == 8 && node.node_value.as_deref() == Some("inside"))
    );
    assert_eq!(document.visible_text(1024), ("Visible".into(), false));
    assert_eq!(
        document.element_inner_html(root, 1024),
        "<!DOCTYPE html PUBLIC \"public-id\" \"system-id\"><!--before--><html><head></head><body><!--inside-->Visible</body></html>"
    );
}

#[test]
fn javascript_inner_html_uses_the_contextual_html5ever_tree_for_svg() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("svg-fragment-parser-test")
        .expect("native JavaScript runtime must construct");
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default()).unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
                    svg.innerHTML = "<foreignObject><p id='html-child'>html</p></foreignObject><linearGradient id='gradient'></linearGradient>";
                    const foreignObject = svg.firstElementChild;
                    const htmlChild = foreignObject.firstElementChild;
                    const gradient = svg.lastElementChild;
                    document.querySelector("#root").appendChild(svg);
                    return [
                        foreignObject.localName === "foreignObject",
                        foreignObject.namespaceURI === "http://www.w3.org/2000/svg",
                        htmlChild.namespaceURI === "http://www.w3.org/1999/xhtml",
                        gradient.localName === "linearGradient",
                        gradient.namespaceURI === "http://www.w3.org/2000/svg",
                    ];
                })()"##,
                &document,
                "fixture://svg-fragment-context.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("SVG fragment context must use html5ever namespace rules");

    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("the contextual SVG fragment must commit unchanged");
    let gradient = document.find_element_by_id("gradient").unwrap();
    assert_eq!(
        document.node(gradient).and_then(NativeNode::element_name),
        Some("linearGradient")
    );
    assert_eq!(
        document.node(gradient).and_then(NativeNode::namespace_uri),
        Some("http://www.w3.org/2000/svg")
    );
}

#[test]
fn javascript_inner_html_uses_rawtext_script_and_plaintext_contexts_same_turn() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("text-state-fragment-test")
        .expect("native JavaScript runtime must construct");
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default()).unwrap();
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const cases = [
                        ["textarea", "one<b>two</textarea><i>three", "one<b>twothree", "i"],
                        ["style", "one<b>two</style><i>three", "one<b>twothree", "i"],
                        ["script", "one<b>two</script><i>three", "one<b>twothree", "i"],
                        ["plaintext", "one<b>two</plaintext><i>three", "one<b>two</plaintext><i>three", null],
                    ];
                    return cases.map(([name, source, expectedText, expectedElement]) => {
                        const element = document.createElement(name);
                        element.setAttribute("id", name);
                        element.innerHTML = source;
                        const actualElement = element.firstElementChild;
                        document.querySelector("#root").appendChild(element);
                        return element.textContent === expectedText
                            && (actualElement ? actualElement.localName : null) === expectedElement;
                    });
                })()"##,
                &document,
                "fixture://text-state-fragment.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("fragment tokenization must follow each context's tokenizer state");

    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true])
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("same-turn context fragments must commit through html5ever");
    for name in ["textarea", "style", "script", "plaintext"] {
        let node = document
            .find_element_by_id(name)
            .expect("context element must be committed");
        assert!(document.node(node).is_some());
    }
}

#[test]
fn active_formatting_noahs_ark_caps_equivalent_entries_at_three() {
    let mut document = NativeDocument::parse("<main></main>", &NativeEngineLimits::default())
        .expect("active-formatting host document must parse");
    let parent = document.find_element(document.root, "main").unwrap();
    let mut attributes = BTreeMap::new();
    attributes.insert("class".to_owned(), "same".to_owned());
    let mut active = ActiveFormattingList::new();
    let mut ids = Vec::new();
    for _ in 0..4 {
        let id = document
            .add_node(
                parent,
                NativeNodeKind::Element {
                    name: "b".to_owned(),
                    attributes: attributes.clone(),
                },
                document.max_nodes,
            )
            .unwrap();
        document.raw_node_mut(id).unwrap().state.namespace_uri =
            Some(HTML_NAMESPACE_URI.to_owned());
        push_active_formatting_element(&document, &mut active, id).unwrap();
        ids.push(id);
    }
    assert_eq!(
        active,
        vec![
            ActiveFormattingEntry::Element(ids[1]),
            ActiveFormattingEntry::Element(ids[2]),
            ActiveFormattingEntry::Element(ids[3]),
        ]
    );
}

#[test]
fn raw_text_and_rcdata_elements_do_not_create_nested_semantic_nodes() {
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse(
            "<ScRiPt>const markup = '<button id=\"fake-script\">Fake</button>';</SCRIPT><STYLE><button id=\"fake-style\">Fake</button></style><title>Doc &amp; <b>Title</b></TITLE><textarea>&lt;button id='fake-textarea'&gt;Fake&lt;/button&gt;</textarea><button id='real'>Real</button>",
            &limits,
        )
        .unwrap();

    assert_eq!(document.title(1024), ("Doc & <b>Title</b>".into(), false));
    let nodes = document.semantic_nodes();
    assert_eq!(nodes.iter().filter(|node| node.role == "button").count(), 1);
    let textarea = nodes.iter().find(|node| node.role == "textbox").unwrap();
    assert_eq!(textarea.name, "<button id='fake-textarea'>Fake</button>");
    assert_eq!(
        nodes
            .iter()
            .find(|node| node.name == "Real")
            .map(|node| node.role.as_str()),
        Some("button")
    );
}

#[test]
fn unterminated_raw_text_consumes_the_bounded_remainder() {
    let document = NativeDocument::parse(
        "<script><button id='fake'>Fake</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();

    assert!(document.semantic_nodes().is_empty());
}

#[test]
fn malformed_markup_recovers_without_partial_elements() {
    let document = NativeDocument::parse(
        "<!--unterminated<!unknown declaration<div title='unfinished>",
        &NativeEngineLimits::default(),
    )
    .unwrap();

    assert!(document.semantic_nodes().is_empty());
    let root_children = document.node(document.root()).unwrap().children();
    assert_eq!(root_children.len(), 2);
    assert!(matches!(
        document.node(root_children[0]).map(NativeNode::kind),
        Some(NativeNodeKind::Comment(value)) if value == "unterminated<!unknown declaration<div title='unfinished>"
    ));
    assert!(matches!(
        document.node(root_children[1]).map(NativeNode::kind),
        Some(NativeNodeKind::Element { name, .. }) if name == "html"
    ));
    assert!(document.find_element_by_id("unfinished").is_none());
}

#[test]
fn duplicate_html_attributes_keep_the_first_value_and_late_doctype_is_ignored() {
    let document = NativeDocument::parse(
        "<!doctype html><button ID='first' id='second'>Action</button><!doctype svg>",
        &NativeEngineLimits::default(),
    )
    .unwrap();

    let root_children = document.node(document.root()).unwrap().children();
    assert_eq!(
        root_children
            .iter()
            .filter(|id| matches!(
                document.node(**id).map(NativeNode::kind),
                Some(NativeNodeKind::DocumentType { .. })
            ))
            .count(),
        1
    );
    let button = document
        .node(document.resolve_target("role=button[name=Action]").unwrap())
        .unwrap();
    assert_eq!(button.attribute("id"), Some("first"));
}

#[test]
fn html_input_stream_newlines_are_normalized_across_parser_routes() {
    let markup = concat!(
        "<section id='newline-zone' data-lines='attr\r\npair and lone\rreturn'>",
        "<p id='normal'>text\r\npair and lone\rreturn</p>",
        "<!--comment\r\npair and lone\rreturn-->",
        "<script id='raw'>raw\r\npair and lone\rreturn</script>",
        "<textarea id='rcdata'>value\r\npair and lone\rreturn</textarea>",
        "</section>",
    );
    let expected = serde_json::json!([
        "attr\npair and lone\nreturn",
        "text\npair and lone\nreturn",
        "comment\npair and lone\nreturn",
        "raw\npair and lone\nreturn",
        "value\npair and lone\nreturn"
    ]);
    let assert_native_tree = |tree: &NativeDocument| {
        let zone = tree
            .find_element_by_id("newline-zone")
            .expect("newline fixture zone must exist");
        let zone_node = tree.node(zone).expect("newline fixture zone must resolve");
        assert_eq!(
            zone_node.attribute("data-lines"),
            Some("attr\npair and lone\nreturn")
        );
        let text_content = |element_id: &str| {
            let element = tree
                .find_element_by_id(element_id)
                .unwrap_or_else(|| panic!("newline fixture element {element_id} must exist"));
            tree.node(element)
                .expect("newline fixture element must resolve")
                .children()
                .iter()
                .find_map(|child| match tree.node(*child).map(NativeNode::kind) {
                    Some(NativeNodeKind::Text(value)) => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("newline fixture element {element_id} has no text"))
        };
        let comment = zone_node
            .children()
            .iter()
            .find_map(|child| match tree.node(*child).map(NativeNode::kind) {
                Some(NativeNodeKind::Comment(value)) => Some(value.as_str()),
                _ => None,
            })
            .expect("newline fixture comment must exist");
        assert_eq!(
            serde_json::json!([
                zone_node.attribute("data-lines").unwrap(),
                text_content("normal"),
                comment,
                text_content("raw"),
                text_content("rcdata")
            ]),
            expected
        );
    };

    let direct = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("document parser must preprocess HTML newlines");
    assert_native_tree(&direct);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("html-newline-preprocess-test")
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
                "https://example.test/html-newline.html",
                "text/html",
            );
            const summarize = (tree) => {
                const zone = tree.querySelector("#newline-zone");
                const comment = Array.from(zone.childNodes).find((node) => node.nodeType === 8);
                return [
                    zone.getAttribute("data-lines"),
                    tree.querySelector("#normal").textContent,
                    comment && comment.data,
                    tree.querySelector("#raw").textContent,
                    tree.querySelector("#rcdata").textContent,
                ];
            };
            return [summarize(root), summarize(response)];
        })()"##
        .replace("__HTML_MARKUP__", &serde_json::to_string(markup).unwrap());
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://html-newline-preprocess.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("same-turn and XHR parsers must preprocess HTML newlines");
    assert_eq!(evaluation.value, serde_json::json!([expected, expected]));

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must preprocess HTML newlines");
    assert_native_tree(&document);
}

#[test]
fn html_tree_builder_recovers_from_missing_attribute_values() {
    let document = NativeDocument::parse("\r\n<div value=>", &NativeEngineLimits::default())
        .expect("HTML tree construction recovers from a missing attribute value");
    assert!(
        document
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code == NativeDiagnosticCode::MalformedHtml })
    );
}

#[test]
fn html_null_characters_follow_tokenizer_and_tree_builder_context() {
    let markup = concat!(
        "<section id='zone' data-null='attr\0literal&#0;reference&\0#0;'>",
        "<p id='body-data'>left\0right&#0;reference&\0#0;tail</p>",
        "<!--comment\0literal &#0;reference-->",
        "<!bogus\0literal &#0;reference>",
        "<script id='script-data'>script\0literal&#0;raw-reference</script>",
        "<style id='style-data'>style\0literal</style>",
        "<textarea id='rcdata'>rc\0data&#0;reference</textarea>",
        "<svg><text id='svg-data'>svg\0literal&#0;reference&\0#0;tail</text>",
        "<foreignObject id='svg-html-data'>foreign\0object</foreignObject></svg>",
        "<math><mtext id='math-html-data'>math\0text</mtext>",
        "<mi id='math-mi-data'>math\0mi</mi></math>",
        "<math><annotation-xml id='math-annotation-html' encoding='application/xhtml+xml'>anno\0xml</annotation-xml></math>",
        "</section>",
    );
    let expected = serde_json::json!({
        "values": [
            "attr\u{FFFD}literal\u{FFFD}reference&\u{FFFD}#0;",
            "leftright\u{FFFD}reference&#0;tail",
            "script\u{FFFD}literal&#0;raw-reference",
            "style\u{FFFD}literal",
            "rc\u{FFFD}data\u{FFFD}reference",
            "svg\u{FFFD}literal\u{FFFD}reference&\u{FFFD}#0;tail",
            "foreignobject",
            "mathtext",
            "mathmi",
            "annoxml",
        ],
        "comments": [
            "comment\u{FFFD}literal &#0;reference",
            "bogus\u{FFFD}literal &#0;reference",
        ],
        "parents": [
            "section", "section", "section", "section", "svg", "svg", "math", "math", "math",
        ]
    });
    let summarize_native = |tree: &NativeDocument| {
        let text_content = |element_id: &str| {
            let element = tree
                .find_element_by_id(element_id)
                .unwrap_or_else(|| panic!("null fixture element {element_id} must exist"));
            tree.node(element)
                .expect("null fixture element must resolve")
                .children()
                .iter()
                .find_map(|child| match tree.node(*child).map(NativeNode::kind) {
                    Some(NativeNodeKind::Text(value)) => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("null fixture element {element_id} has no text"))
        };
        let zone = tree
            .find_element_by_id("zone")
            .expect("null fixture section must exist");
        let zone_node = tree.node(zone).expect("null fixture section must resolve");
        let comments = zone_node
            .children()
            .iter()
            .filter_map(|child| match tree.node(*child).map(NativeNode::kind) {
                Some(NativeNodeKind::Comment(value)) => Some(value.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let ids = [
            "body-data",
            "script-data",
            "style-data",
            "rcdata",
            "svg-data",
            "svg-html-data",
            "math-html-data",
            "math-mi-data",
            "math-annotation-html",
        ];
        let parents = ids
            .iter()
            .map(|element_id| {
                let element = tree.find_element_by_id(element_id).unwrap();
                let parent = tree.node(element).and_then(NativeNode::parent).unwrap();
                tree.node(parent)
                    .and_then(NativeNode::element_name)
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "values": [
                zone_node.attribute("data-null").unwrap(),
                text_content("body-data"),
                text_content("script-data"),
                text_content("style-data"),
                text_content("rcdata"),
                text_content("svg-data"),
                text_content("svg-html-data"),
                text_content("math-html-data"),
                text_content("math-mi-data"),
                text_content("math-annotation-html"),
            ],
            "comments": comments,
            "parents": parents,
        })
    };

    let direct = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("document parser must apply contextual HTML null handling");
    assert_eq!(summarize_native(&direct), expected);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("html-null-context-test")
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
                "https://example.test/html-null-context.html",
                "text/html",
            );
            const summarize = (tree) => {
                const zone = tree.querySelector("#zone");
                const comments = Array.from(zone.childNodes)
                    .filter((node) => node.nodeType === 8)
                    .map((node) => node.data);
                const ids = [
                    "body-data", "script-data", "style-data", "rcdata",
                    "svg-data", "svg-html-data", "math-html-data", "math-mi-data",
                    "math-annotation-html",
                ];
                return {
                    values: [
                        zone.getAttribute("data-null"),
                        tree.querySelector("#body-data").textContent,
                        tree.querySelector("#script-data").textContent,
                        tree.querySelector("#style-data").textContent,
                        tree.querySelector("#rcdata").textContent,
                        tree.querySelector("#svg-data").textContent,
                        tree.querySelector("#svg-html-data").textContent,
                        tree.querySelector("#math-html-data").textContent,
                        tree.querySelector("#math-mi-data").textContent,
                        tree.querySelector("#math-annotation-html").textContent,
                    ],
                    comments,
                    parents: ids.map((id) => tree.querySelector(`#${id}`).parentElement.localName),
                };
            };
            return [summarize(root), summarize(response)];
        })()"##
        .replace("__HTML_MARKUP__", &serde_json::to_string(markup).unwrap());
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://html-null-context.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("same-turn and XHR parsers must apply contextual HTML null handling");
    assert_eq!(evaluation.value, serde_json::json!([expected, expected]));

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must apply contextual HTML null handling");
    assert_eq!(summarize_native(&document), expected);
}

#[test]
fn html_special_text_modes_respect_element_namespace_across_parser_routes() {
    let markup = concat!(
        "<section id='zone'><svg id='svg-root'>",
        "<title id='svg-title'><span id='svg-title-span'>title</span></title>",
        "<style id='svg-style'><g id='svg-style-g'>style</g></style>",
        "<script id='svg-script'><g id='svg-script-g'>script</g></script>",
        "<xmp id='svg-xmp'><g id='svg-xmp-g'>xmp</g></xmp>",
        "<iframe id='svg-iframe'><g id='svg-iframe-g'>iframe</g></iframe>",
        "</svg><math id='math-root'>",
        "<textarea id='math-textarea'><mi id='math-textarea-mi'>mi</mi></textarea>",
        "<title id='math-title'><mrow id='math-title-row'>row</mrow></title>",
        "<noembed id='math-noembed'><mi id='math-noembed-mi'>noembed</mi></noembed>",
        "</math></section>",
    );
    let expected_nodes = [
        ("svg-root", SVG_NAMESPACE_URI, "zone"),
        ("svg-title", SVG_NAMESPACE_URI, "svg-root"),
        ("svg-title-span", HTML_NAMESPACE_URI, "svg-title"),
        ("svg-style", SVG_NAMESPACE_URI, "svg-root"),
        ("svg-style-g", SVG_NAMESPACE_URI, "svg-style"),
        ("svg-script", SVG_NAMESPACE_URI, "svg-root"),
        ("svg-script-g", SVG_NAMESPACE_URI, "svg-script"),
        ("svg-xmp", SVG_NAMESPACE_URI, "svg-root"),
        ("svg-xmp-g", SVG_NAMESPACE_URI, "svg-xmp"),
        ("svg-iframe", SVG_NAMESPACE_URI, "svg-root"),
        ("svg-iframe-g", SVG_NAMESPACE_URI, "svg-iframe"),
        ("math-root", MATHML_NAMESPACE_URI, "zone"),
        ("math-textarea", MATHML_NAMESPACE_URI, "math-root"),
        ("math-textarea-mi", MATHML_NAMESPACE_URI, "math-textarea"),
        ("math-title", MATHML_NAMESPACE_URI, "math-root"),
        ("math-title-row", MATHML_NAMESPACE_URI, "math-title"),
        ("math-noembed", MATHML_NAMESPACE_URI, "math-root"),
        ("math-noembed-mi", MATHML_NAMESPACE_URI, "math-noembed"),
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
                    panic!("special-text fixture element {element_id} must exist")
                });
                let node = tree
                    .node(element)
                    .expect("special-text fixture element must resolve");
                let parent_id = node
                    .parent()
                    .and_then(|parent| tree.node(parent))
                    .and_then(|parent| parent.attribute("id"))
                    .expect("special-text fixture parent must have an id");
                serde_json::json!([node.namespace_uri().unwrap(), parent_id])
            })
            .collect::<Vec<_>>()
    };

    let direct = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("document parser must keep special text modes namespace-aware");
    assert_eq!(summarize_native(&direct), expected);

    let runtime = NativeJavaScriptRuntime::new_with_context_id("html-special-text-context")
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
                "https://example.test/html-special-text-context.html",
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
            "fixture://html-special-text-context.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("fragment projection and XHR document must respect foreign text modes");
    assert_eq!(evaluation.value, serde_json::json!([expected, expected]));

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must respect foreign text modes");
    assert_eq!(summarize_native(&document), expected);
}

#[test]
fn html_rcdata_fragment_contexts_match_same_turn_and_committed_trees() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("rcdata-fragment-context")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
            "<svg id='svg-root'><title id='svg-title'></title></svg><title id='html-title'></title><textarea id='textarea'></textarea>",
            &NativeEngineLimits::default(),
        )
        .expect("RCDATA fragment context document must parse");
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const svgTitle = document.querySelector("#svg-title");
                    const htmlTitle = document.querySelector("#html-title");
                    const textarea = document.querySelector("#textarea");
                    svgTitle.innerHTML = "<span id='svg-literal'>SVG &amp; text</span>";
                    const svgLiteral = [
                        svgTitle.textContent,
                        document.querySelector("#svg-literal") === null,
                    ];
                    htmlTitle.innerHTML = "<b id='html-literal'>HTML &amp; text</b>";
                    const htmlLiteral = [
                        htmlTitle.textContent,
                        document.querySelector("#html-literal") === null,
                    ];
                    textarea.innerHTML = "<i id='textarea-literal'>TA &amp; text</i>";
                    const textareaLiteral = [
                        textarea.textContent,
                        document.querySelector("#textarea-literal") === null,
                    ];
                    svgTitle.innerHTML = "prefix &amp; \u0000 </title/><span id='after-close'>after</span>";
                    const afterClose = document.querySelector("#after-close");
                    return [
                        svgLiteral,
                        htmlLiteral,
                        textareaLiteral,
                        [svgTitle.textContent, afterClose.namespaceURI, afterClose.parentElement === svgTitle],
                    ];
                })()"##,
                &document,
                "fixture://rcdata-fragment-context.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("RCDATA fragment context must project immediately");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            ["SVG & text", false],
            ["<b id='html-literal'>HTML & text</b>", true],
            ["<i id='textarea-literal'>TA & text</i>", true],
            ["prefix &  after", HTML_NAMESPACE_URI, true]
        ])
    );

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must match the immediate RCDATA projection");
    for (element_id, expected_text) in [
        ("html-title", "<b id='html-literal'>HTML & text</b>"),
        ("textarea", "<i id='textarea-literal'>TA & text</i>"),
    ] {
        let element_id = document
            .find_element_by_id(element_id)
            .expect("RCDATA context element must remain attached");
        let element = document
            .node(element_id)
            .expect("RCDATA context element must resolve");
        assert_eq!(element.children().len(), 1);
        assert_eq!(
            document.node(element.children()[0]).unwrap().kind(),
            &NativeNodeKind::Text(expected_text.to_owned())
        );
    }

    let svg_title_id = document
        .find_element_by_id("svg-title")
        .expect("SVG title context must remain attached");
    let svg_title = document.node(svg_title_id).unwrap();
    assert_eq!(svg_title.children().len(), 2);
    assert_eq!(
        document.node(svg_title.children()[0]).unwrap().kind(),
        &NativeNodeKind::Text("prefix &  ".to_owned())
    );
    let after_close_id = document
        .find_element_by_id("after-close")
        .expect("markup after the RCDATA context end tag must parse");
    let after_close = document.node(after_close_id).unwrap();
    assert_eq!(after_close.namespace_uri(), Some(HTML_NAMESPACE_URI));
    assert_eq!(after_close.parent(), Some(svg_title_id));
    assert!(document.find_element_by_id("svg-literal").is_none());
    assert!(document.find_element_by_id("html-literal").is_none());
    assert!(document.find_element_by_id("textarea-literal").is_none());
}

#[test]
fn html_rawtext_elements_remain_literal_across_document_routes() {
    let raw_text_elements = ["style", "xmp", "iframe", "noembed", "noframes", "noscript"];
    let raw_text = |name: &str| format!("<b id='fake-{name}'>A &amp; {} Z</b>", '\u{FFFD}');
    let markup = raw_text_elements
        .iter()
        .map(|name| format!("<{name} id='{name}'>{}</{name}>", raw_text(name)))
        .collect::<String>();
    let expected = raw_text_elements
        .iter()
        .map(|name| {
            let text = raw_text(name);
            serde_json::json!([text.clone(), text, true])
        })
        .collect::<Vec<_>>();
    let mut expected_inert_document = expected.clone();
    expected_inert_document[5] =
        serde_json::json!(["A & � Z", "<b id=\"fake-noscript\">A &amp; � Z</b>", false]);

    let direct = NativeDocument::parse(&markup, &NativeEngineLimits::default())
        .expect("HTML raw-text elements must parse as literal text");
    for name in raw_text_elements {
        let element_id = direct
            .find_element_by_id(name)
            .expect("raw-text element must remain in the document");
        let element = direct.node(element_id).unwrap();
        assert_eq!(
            element.children().len(),
            1,
            "{name} must have one text child"
        );
        assert_eq!(
            direct.node(element.children()[0]).unwrap().kind(),
            &NativeNodeKind::Text(raw_text(name))
        );
        assert!(direct.find_element_by_id(&format!("fake-{name}")).is_none());
        assert_eq!(
            direct.element_inner_html(element_id, 1 << 20),
            raw_text(name)
        );
    }

    let runtime = NativeJavaScriptRuntime::new_with_context_id("rawtext-document-routes")
        .expect("native JavaScript runtime must construct");
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default())
            .expect("fragment host document must parse");
    let script = r##"(() => {
            const names = __NAMES__;
            const markup = __MARKUP__;
            const root = document.querySelector("#root");
            root.innerHTML = markup;
            const response = globalThis.__glassParseHtmlDocument(
                markup,
                "https://example.test/rawtext-document-routes.html",
                "text/html",
            );
            const summarize = (tree) => names.map((name) => {
                const element = tree.querySelector(`#${name}`);
                return [
                    element.textContent,
                    element.innerHTML,
                    tree.querySelector(`#fake-${name}`) === null,
                ];
            });
            return [summarize(root), summarize(response)];
        })()"##
        .replace(
            "__NAMES__",
            &serde_json::to_string(&raw_text_elements).unwrap(),
        )
        .replace("__MARKUP__", &serde_json::to_string(&markup).unwrap());
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://rawtext-document-routes.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("fragment projection and XHR document must preserve RAWTEXT");
    assert_eq!(
        evaluation.value,
        serde_json::json!([expected, expected_inert_document])
    );

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must preserve RAWTEXT");
    let root_id = document.find_element_by_id("root").unwrap();
    let root = document.node(root_id).unwrap();
    for name in raw_text_elements {
        let element_id = document.find_element_by_id(name).unwrap();
        let element = document.node(element_id).unwrap();
        assert_eq!(
            element.children().len(),
            1,
            "{name} must have one text child"
        );
        assert_eq!(
            document.node(element.children()[0]).unwrap().kind(),
            &NativeNodeKind::Text(raw_text(name))
        );
        assert_eq!(
            document.element_inner_html(element_id, 1 << 20),
            raw_text(name)
        );
        assert!(
            document
                .find_element_by_id(&format!("fake-{name}"))
                .is_none()
        );
        assert_eq!(element.parent(), Some(root_id));
    }
    assert_eq!(root.children().len(), raw_text_elements.len());
}

#[test]
fn html_rawtext_fragment_contexts_match_same_turn_and_committed_trees() {
    let raw_text_elements = ["style", "xmp", "iframe", "noembed", "noframes", "noscript"];
    let contexts = raw_text_elements
        .iter()
        .map(|name| format!("<{name} id='{name}'></{name}>"))
        .collect::<String>();
    let mut document = NativeDocument::parse(
        &format!("<main id='root'>{contexts}</main>"),
        &NativeEngineLimits::default(),
    )
    .expect("raw-text context document must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("rawtext-fragment-context")
        .expect("native JavaScript runtime must construct");
    let script = r##"(() => {
            const names = __NAMES__;
            const results = names.map((name) => {
                const target = document.querySelector(`#${name}`);
                target.innerHTML = "<b id='fake-" + name + "'>A &amp; \u0000 Z</b></" + name + "/>"
                    + "<span id='after-" + name + "'>tail</span>";
                const after = target.querySelector(`#after-${name}`);
                return [
                    target.textContent,
                    target.querySelector(`#fake-${name}`) === null,
                    after.namespaceURI,
                    after.parentElement === target,
                    target.innerHTML,
                ];
            });
            const xmp = document.querySelector("#xmp");
            xmp.innerHTML = "EOF &amp; <b id='eof-fake'>tail</b>";
            return [results, [xmp.textContent, xmp.innerHTML, xmp.querySelector("#eof-fake") === null]];
        })()"##
            .replace(
                "__NAMES__",
                &serde_json::to_string(&raw_text_elements).unwrap(),
            );
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://rawtext-fragment-context.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("RAWTEXT fragment contexts must project immediately");

    let expected_context_results = raw_text_elements
        .iter()
        .map(|name| {
            let text = format!("<b id='fake-{name}'>A &amp; {} Z</b>", '\u{FFFD}');
            serde_json::json!([
                format!("{text}tail"),
                true,
                HTML_NAMESPACE_URI,
                true,
                format!("{text}<span id=\"after-{name}\">tail</span>"),
            ])
        })
        .collect::<Vec<_>>();
    let unterminated = "EOF &amp; <b id='eof-fake'>tail</b>";
    assert_eq!(
        evaluation.value,
        serde_json::json!([expected_context_results, [unterminated, unterminated, true],])
    );

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must match RAWTEXT projection");
    for name in raw_text_elements {
        let element_id = document.find_element_by_id(name).unwrap();
        let element = document.node(element_id).unwrap();
        if name == "xmp" {
            assert_eq!(element.children().len(), 1);
            assert_eq!(
                document.node(element.children()[0]).unwrap().kind(),
                &NativeNodeKind::Text(unterminated.to_owned())
            );
            assert_eq!(
                document.element_inner_html(element_id, 1 << 20),
                unterminated
            );
            continue;
        }
        let literal = format!("<b id='fake-{name}'>A &amp; {} Z</b>", '\u{FFFD}');
        assert_eq!(
            element.children().len(),
            2,
            "{name} suffix must parse normally"
        );
        assert_eq!(
            document.node(element.children()[0]).unwrap().kind(),
            &NativeNodeKind::Text(literal.clone())
        );
        let suffix_id = document
            .find_element_by_id(&format!("after-{name}"))
            .unwrap();
        let suffix = document.node(suffix_id).unwrap();
        assert_eq!(suffix.parent(), Some(element_id));
        assert_eq!(suffix.namespace_uri(), Some(HTML_NAMESPACE_URI));
        assert_eq!(
            document.element_inner_html(element_id, 1 << 20),
            format!("{literal}<span id=\"after-{name}\">tail</span>")
        );
        assert!(
            document
                .find_element_by_id(&format!("fake-{name}"))
                .is_none()
        );
    }
    assert!(document.find_element_by_id("eof-fake").is_none());
}

#[test]
fn html_plaintext_fragment_context_consumes_source_to_eof() {
    let source = "before &amp; \0 <b id='fake'>inside</b></plaintext><span id='suffix'>tail</span>\r\nlast\rend";
    let expected = "before &amp; \u{FFFD} <b id='fake'>inside</b></plaintext><span id='suffix'>tail</span>\nlast\nend";
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default())
            .expect("fragment host document must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("plaintext-fragment-context")
        .expect("native JavaScript runtime must construct");
    let script = format!(
        "(() => {{\
                const target = document.createElement('plaintext');\
                target.setAttribute('id', 'plain');\
                document.querySelector('#root').appendChild(target);\
                target.innerHTML = {};\
                return [target.textContent, target.innerHTML, target.childNodes.length,\
                    target.firstChild.nodeType, target.children.length,\
                    target.querySelector('#fake') === null,\
                    target.querySelector('#suffix') === null];\
            }})()",
        serde_json::to_string(source).expect("plaintext source must serialize")
    );
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://plaintext-fragment-context.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("plaintext fragment context must project immediately");

    assert_eq!(
        evaluation.value,
        serde_json::json!([expected, expected, 1, 3, 0, true, true])
    );

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust fragment commit must match the PLAINTEXT projection");
    let target_id = document
        .find_element_by_id("plain")
        .expect("plaintext target must remain attached");
    let target = document.node(target_id).unwrap();
    assert_eq!(target.children().len(), 1);
    assert_eq!(
        document.node(target.children()[0]).unwrap().kind(),
        &NativeNodeKind::Text(expected.to_owned())
    );
    assert_eq!(document.element_inner_html(target_id, 1 << 20), expected);
    assert!(document.find_element_by_id("fake").is_none());
    assert!(document.find_element_by_id("suffix").is_none());
}

#[test]
fn html_script_data_end_scanner_handles_false_and_double_escaped_closers() {
    let source = "<!--<script>double</script>escaped--></SCRIPT >suffix";
    let close = find_script_data_end(source, 0).expect("final script close must be found");
    assert_eq!(&source[close..], "</SCRIPT >suffix");

    let source = "<!--escaped </script>suffix";
    let close = find_script_data_end(source, 0).expect("escaped script close must be found");
    assert_eq!(&source[close..], "</script>suffix");

    let source = "before </scriptx> after </script>";
    let close = find_script_data_end(source, 0).expect("appropriate script close must be found");
    assert_eq!(&source[close..], "</script>");

    assert!(find_script_data_end("before </script", 0).is_none());
    assert!(find_script_data_end("no closing script", 0).is_none());
}

#[test]
fn html_script_data_contexts_match_all_parser_routes() {
    let markup = "<script id='double'><!--<script>double &amp; </script>escaped\0--></script><span id='after-double'>double tail</span><script id='escaped'><!-- escaped </SCRIPT ><b id='after-escaped'>escaped tail</b><script id='prefix'>before </scriptx> after</script><u id='after-prefix'>prefix tail</u><script id='fragment'></script>";
    let double_text = format!("<!--<script>double &amp; </script>escaped{}-->", '\u{FFFD}');
    let expected = serde_json::json!([
        [double_text, double_text, "double tail", true],
        ["<!-- escaped ", "<!-- escaped ", "escaped tail", true],
        [
            "before </scriptx> after",
            "before </scriptx> after",
            "prefix tail",
            true
        ]
    ]);
    let document_markup =
        format!("<main id='root'>{markup}<script id='unterminated'></script></main>");
    let mut document = NativeDocument::parse(&document_markup, &NativeEngineLimits::default())
        .expect("script-data fixture document must parse");
    let direct_root = document.find_element_by_id("root").unwrap();
    for (id, text, after_id) in [
        ("double", expected[0][0].as_str().unwrap(), "after-double"),
        ("escaped", "<!-- escaped ", "after-escaped"),
        ("prefix", "before </scriptx> after", "after-prefix"),
    ] {
        let script_id = document.find_element_by_id(id).unwrap();
        let script = document.node(script_id).unwrap();
        assert_eq!(script.children().len(), 1, "{id} has one script text node");
        assert_eq!(
            document.node(script.children()[0]).unwrap().kind(),
            &NativeNodeKind::Text(text.to_owned())
        );
        assert_eq!(document.element_inner_html(script_id, 1 << 20), text);
        let after_id = document.find_element_by_id(after_id).unwrap();
        assert_eq!(document.node(after_id).unwrap().parent(), Some(direct_root));
    }

    let fragment_source =
        "inside<!--<script>double</script>after--></SCRIPT ><i id='fragment-tail'>tail</i>";
    let unterminated_source = "EOF &amp; </script";
    let runtime = NativeJavaScriptRuntime::new_with_context_id("script-data-fragment-context")
        .expect("native JavaScript runtime must construct");
    let script = format!(
        "(() => {{\
                const markup = {};\
                const fragment = document.querySelector('#fragment');\
                fragment.innerHTML = {};\
                const unterminated = document.querySelector('#unterminated');\
                unterminated.innerHTML = {};\
                const summarize = (scope) => {{\
                    const host = scope.querySelector('#root');\
                    return ['double', 'escaped', 'prefix'].map((id) => {{\
                        const element = scope.querySelector(`#${{id}}`);\
                        const after = scope.querySelector(`#after-${{id}}`);\
                        return [element.textContent, element.innerHTML,\
                            after.textContent, after.parentElement === host];\
                    }});\
                }};\
                const response = __glassParseHtmlDocument(\
                    markup, 'https://example.test/script-data-routes.html', 'text/html');\
                return [summarize(document), summarize(response),\
                    [fragment.textContent, fragment.innerHTML,\
                        fragment.querySelector('#fragment-tail').parentElement === fragment],\
                    [unterminated.textContent, unterminated.innerHTML,\
                        unterminated.childNodes.length, unterminated.children.length]];\
            }})()",
        serde_json::to_string(&document_markup).unwrap(),
        serde_json::to_string(fragment_source).unwrap(),
        serde_json::to_string(unterminated_source).unwrap(),
    );
    let evaluation = runtime
        .evaluate(
            &script,
            &document,
            "fixture://script-data-fragment-context.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("script-data parser routes must project consistently");
    let fragment_text = "inside<!--<script>double</script>after-->";
    let fragment_html = format!("{fragment_text}<i id=\"fragment-tail\">tail</i>");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            expected,
            expected,
            [format!("{fragment_text}tail"), fragment_html, true],
            [unterminated_source, unterminated_source, 1, 0]
        ])
    );

    document
        .apply_script_commands(&evaluation.commands)
        .expect("Rust script fragment commit must match same-turn projection");
    let committed_root = document.find_element_by_id("root").unwrap();
    for (id, text) in [
        ("double", expected[0][0].as_str().unwrap()),
        ("escaped", "<!-- escaped "),
        ("prefix", "before </scriptx> after"),
    ] {
        let script_id = document.find_element_by_id(id).unwrap();
        let script = document.node(script_id).unwrap();
        assert_eq!(script.children().len(), 1);
        assert_eq!(
            document.node(script.children()[0]).unwrap().kind(),
            &NativeNodeKind::Text(text.to_owned())
        );
        assert_eq!(document.element_inner_html(script_id, 1 << 20), text);
        let after_id = document.find_element_by_id(&format!("after-{id}")).unwrap();
        assert_eq!(
            document.node(after_id).unwrap().parent(),
            Some(committed_root)
        );
    }
    let fragment_id = document.find_element_by_id("fragment").unwrap();
    let fragment = document.node(fragment_id).unwrap();
    assert_eq!(
        fragment.children().len(),
        2,
        "fragment children: {:?}",
        fragment
            .children()
            .iter()
            .map(|child| document.node(*child).unwrap().kind())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        document.node(fragment.children()[0]).unwrap().kind(),
        &NativeNodeKind::Text(fragment_text.to_owned())
    );
    assert_eq!(
        document.element_inner_html(fragment_id, 1 << 20),
        fragment_html
    );
    let unterminated_id = document.find_element_by_id("unterminated").unwrap();
    let unterminated = document.node(unterminated_id).unwrap();
    assert_eq!(unterminated.children().len(), 1);
    assert_eq!(
        document.node(unterminated.children()[0]).unwrap().kind(),
        &NativeNodeKind::Text(unterminated_source.to_owned())
    );
    assert_eq!(
        document.element_inner_html(unterminated_id, 1 << 20),
        unterminated_source
    );

    let frame_parent = NativeDocument::parse(
        "<iframe id='frame'></iframe>",
        &NativeEngineLimits::default(),
    )
    .expect("frame parent document must parse");
    let frame_child = NativeDocument::parse(
            "<script id='frame-script'><!--<script>double</script>escaped   &amp;</script><span id='frame-after'>tail</span>",
            &NativeEngineLimits::default(),
        )
        .expect("frame child document must parse");
    let frame_runtime = NativeJavaScriptRuntime::new_with_context_id("script-data-frame-context")
        .expect("native frame JavaScript runtime must construct");
    frame_runtime.set_frame_script_bindings(vec![NativeFrameScriptBinding {
        node_index: frame_parent.find_element_by_id("frame").unwrap().index(),
        frame_id: "script-data-child".into(),
        url: "https://example.test/script-data-child.html".into(),
        origin: "https://example.test".into(),
        generation: 1,
        revision: 1,
        same_origin: true,
        document: frame_child.script_snapshot_for_viewport(1 << 20, Viewport::default()),
        children: Vec::new(),
    }]);
    let frame_evaluation = frame_runtime
            .evaluate(
                "(() => { const script = document.querySelector('#frame').contentDocument.querySelector('#frame-script'); return [script.textContent, script.innerHTML]; })()",
                &frame_parent,
                "https://example.test/script-data-parent.html",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("same-origin frame script snapshot must project");
    assert_eq!(
        frame_evaluation.value,
        serde_json::json!([
            "<!--<script>double</script>escaped   &amp;",
            "<!--<script>double</script>escaped   &amp;"
        ])
    );
}

#[test]
fn malformed_quoted_attribute_does_not_emit_partial_start_tag() {
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse("<button title='unfinished>", &limits).unwrap();
    assert!(document.find_element(document.root, "button").is_none());
    assert_eq!(document.visible_text(1024).0, "");
}

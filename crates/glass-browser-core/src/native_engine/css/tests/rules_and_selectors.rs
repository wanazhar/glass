use super::*;

#[test]
fn static_css_imports_ignore_comments_and_strings_and_reject_malformed_rules() {
    let source = r#"/* @import 'comment.css'; */
            .text { content: "@import 'string.css';"; }
            @IMPORT 'theme.css' screen;
            @import url("nested.css") layer(theme);"#;
    let imports = static_css_imports(source).unwrap();
    assert_eq!(
        imports
            .into_iter()
            .map(|import| import.specifier)
            .collect::<Vec<_>>(),
        vec!["theme.css", "nested.css"]
    );
    assert!(static_css_imports("@import 'missing-semicolon.css'").is_err());
    assert!(static_css_imports("@import url(\"unterminated.css); ").is_err());
    assert!(static_css_imports("@import unsupported.css;").is_err());
}

#[test]
fn css_import_conditions_parse_and_match_native_viewport() {
    let imports = static_css_imports(
            "@import 'wide.css' layer(theme) supports((display: grid) and (color: red)) screen and (min-width: 1000px); @import url(narrow.css) screen and (max-width: 600px); @import 'print.css' print;",
        )
        .unwrap();
    assert_eq!(imports.len(), 3);
    assert_eq!(imports[0].layer.as_deref(), Some("theme"));
    assert_eq!(
        imports[0].supports.as_deref(),
        Some("(display: grid) and (color: red)")
    );
    assert_eq!(
        imports[0].media.as_deref(),
        Some("screen and (min-width: 1000px)")
    );
    assert!(css_import_matches(&imports[0], Viewport::default()));
    assert!(!css_import_matches(
        &imports[0],
        Viewport {
            width: 800,
            ..Viewport::default()
        }
    ));
    assert!(!css_import_matches(&imports[1], Viewport::default()));
    assert!(!css_import_matches(&imports[2], Viewport::default()));

    let anonymous = static_css_imports("@import 'a.css' layer; @import 'b.css' layer;").unwrap();
    assert_ne!(anonymous[0].layer, anonymous[1].layer);
}

#[test]
fn inline_media_rules_match_bounded_viewport_features() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#target { color: white; } @media screen and (min-width: 1000px) { #target { color: red; } } @media screen and (max-width: 600px) { #target { color: black; } } @media print { #target { color: red; } }"
                .into(),
        ])
        .expect("media stylesheet");
    let element = node("<div id='target'>Target</div>");

    assert_eq!(
        stylesheet
            .computed_for_with_viewport(&element, Viewport::default())
            .color(),
        Some(NativeColor::RED)
    );
    assert_eq!(
        stylesheet
            .computed_for_with_viewport(
                &element,
                Viewport {
                    width: 800,
                    ..Viewport::default()
                }
            )
            .color(),
        Some(NativeColor::WHITE)
    );
    assert_eq!(
        stylesheet
            .computed_for_with_viewport(
                &element,
                Viewport {
                    width: 500,
                    ..Viewport::default()
                }
            )
            .color(),
        Some(NativeColor::BLACK)
    );
}

#[test]
fn inline_media_rules_filter_custom_properties_by_viewport() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#target { color: var(--accent, white); } @media screen and (min-width: 1000px) { #target { --accent: red; } }"
                .into(),
        ])
        .expect("media custom-property stylesheet");
    let element = node("<div id='target'>Target</div>");

    assert_eq!(
        stylesheet
            .computed_for_with_viewport(&element, Viewport::default())
            .color(),
        Some(NativeColor::RED)
    );
    assert_eq!(
        stylesheet
            .computed_for_with_viewport(
                &element,
                Viewport {
                    width: 800,
                    ..Viewport::default()
                }
            )
            .color(),
        Some(NativeColor::WHITE)
    );
}

#[test]
fn inline_supports_rules_match_supported_declarations_and_composition() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#target { color: white; } @supports (display: grid) { #target { color: red; } } @supports (display: unsupported) { #target { color: black; } } @media screen and (min-width: 1000px) { @supports (display: grid) and (color: red) { #target { color: black; } } }"
                .into(),
        ])
        .expect("supports stylesheet");
    let element = node("<div id='target'>Target</div>");

    assert_eq!(
        stylesheet
            .computed_for_with_viewport(&element, Viewport::default())
            .color(),
        Some(NativeColor::BLACK)
    );
    assert_eq!(
        stylesheet
            .computed_for_with_viewport(
                &element,
                Viewport {
                    width: 800,
                    ..Viewport::default()
                }
            )
            .color(),
        Some(NativeColor::RED)
    );
}

#[test]
fn inline_supports_rules_filter_custom_properties_by_support() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#target { color: var(--accent, white); } @supports (display: grid) { #target { --accent: red; } } @supports (display: unsupported) { #target { --accent: black; } }"
                .into(),
        ])
        .expect("supports custom-property stylesheet");
    let element = node("<div id='target'>Target</div>");

    assert_eq!(
        stylesheet
            .computed_for_with_viewport(&element, Viewport::default())
            .color(),
        Some(NativeColor::RED)
    );
}
#[test]
fn css_url_escape_decoder_handles_hex_simple_and_line_continuation_forms() {
    assert_eq!(
        decode_css_url_value(r"assets/theme\2e css"),
        Some("assets/theme.css".into())
    );
    assert_eq!(
        decode_css_url_value(r"space\20 name\)asset"),
        Some("space name)asset".into())
    );
    let escaped_closing_parenthesis = static_css_imports(r"@import url(foo\)bar.css);").unwrap();
    assert_eq!(escaped_closing_parenthesis[0].specifier, r"foo\)bar.css");
    assert_eq!(
        decode_css_url_value("line\\\ncontinued"),
        Some("linecontinued".into())
    );
    assert_eq!(decode_css_url_value(r"trailing\"), None);
    assert_eq!(decode_css_url_value(r"nul\0"), Some("nul\u{fffd}".into()));
}

#[test]
fn stylesheet_urls_use_the_loaded_file_or_network_sheet_base() {
    let source =
        "#target { background-image: url('../assets/dot.png'); mask-image: url(\"/mask.svg\"); }";
    let network =
        absolutize_stylesheet_urls(source, "http://example.test/page", "/styles/theme/main.css");
    assert!(network.contains("url('http://example.test/styles/assets/dot.png')"));
    assert!(network.contains("url(\"http://example.test/mask.svg\")"));

    let file = absolutize_stylesheet_urls(
        source,
        "file:///tmp/site/page.html",
        "styles/theme/main.css",
    );
    assert!(file.contains("url('file:///tmp/site/styles/assets/dot.png')"));
    assert!(file.contains("url(\"file:///mask.svg\")"));

    assert_eq!(
        absolutize_stylesheet_urls(
            "#target { background-image: url(data:image/png;base64,AAAA); }",
            "http://example.test/page",
            "/styles/main.css",
        ),
        "#target { background-image: url(data:image/png;base64,AAAA); }"
    );
    assert_eq!(
        absolutize_stylesheet_urls(
            source,
            "file:///tmp/site/page.html",
            "https://cdn.test/main.css",
        ),
        source
    );
}

#[test]
fn selector_parser_supports_bounded_compound_and_descendant_selectors() {
    let selector = parse_selector("main .card button[data-state=ready]").unwrap();
    assert_eq!(selector.specificity, 22);
    assert!(parse_selector("button[data-label='ready now']").is_some());
    assert!(parse_selector("[data-token]").is_some());
    assert!(parse_selector("[data-token='alpha beta']").is_some());
    assert!(parse_selector("[data-token~=beta]").is_some());
    assert!(parse_selector("[data-lang|=en]").is_some());
    assert!(parse_selector("[data-prefix^=pre]").is_some());
    assert!(parse_selector("[data-suffix$=suffix]").is_some());
    assert!(parse_selector("[data-text*=middle]").is_some());
    let insensitive = parse_selector("[data-role='BUTTON' i]").unwrap();
    assert_eq!(
        insensitive.compounds[0].attributes[0].case_sensitivity,
        NativeAttributeCaseSensitivity::Insensitive
    );
    let sensitive = parse_selector("[data-role=BUTTON s]").unwrap();
    assert_eq!(
        sensitive.compounds[0].attributes[0].case_sensitivity,
        NativeAttributeCaseSensitivity::Sensitive
    );
    assert!(parse_selector("[data-role='BUTTON'i]").is_none());
    assert!(parse_selector("[data-role='BUTTON' x]").is_none());
    assert!(parse_selector("[data-token~=]").is_none());
    assert!(parse_selector("[data-token?=beta]").is_none());
    assert!(parse_selector("main > button").is_some());
    assert!(parse_selector("main + button").is_some());
    assert!(parse_selector("main ~ button").is_some());
    assert!(parse_selector("main >").is_none());
    assert!(parse_selector("> button").is_none());
    assert!(parse_selector("main ++ button").is_none());
    assert!(parse_selector("button:hover").is_some());
    assert!(parse_selector("div:user-valid").is_some());
    let too_many = ["div"; MAX_SELECTOR_PARTS + 1].join(" ");
    assert!(parse_selector(&too_many).is_none());
    assert_eq!(
        selector_diagnostic_detail(&too_many),
        "selector-too-complex"
    );
}

#[test]
fn selector_parser_supports_bounded_namespace_type_selectors() {
    let svg = parse_selector("svg|circle").unwrap();
    assert_eq!(svg.specificity, 1);
    assert_eq!(svg.compounds[0].tag_namespace, NativeSelectorNamespace::Svg);
    assert_eq!(
        parse_selector("*|circle").unwrap().compounds[0].tag_namespace,
        NativeSelectorNamespace::Any
    );
    assert_eq!(
        parse_selector("|circle").unwrap().compounds[0].tag_namespace,
        NativeSelectorNamespace::NoNamespace
    );
    assert!(parse_selector("svg|*").is_some());
    assert!(parse_selector("foo|circle").is_none());
    assert!(parse_selector("svg|").is_none());
    assert!(parse_selector("*|").is_none());
}

#[test]
fn namespace_statement_parser_supports_bounded_aliases() {
    assert_eq!(
        parse_namespace_statement("@namespace icon url('http://www.w3.org/2000/svg')"),
        Some(Ok(("icon".to_owned(), NativeSelectorNamespace::Svg,)))
    );
    assert_eq!(
        parse_namespace_statement("@namespace url(http://www.w3.org/1999/xlink)"),
        Some(Ok((String::new(), NativeSelectorNamespace::Xlink)))
    );
    assert_eq!(
        parse_namespace_statement("@namespace icon url(http://example.com/custom)"),
        Some(Err("namespace-uri"))
    );
    assert_eq!(
        parse_namespace_statement("@namespacefoo url(http://www.w3.org/2000/svg)"),
        None
    );
}

#[test]
fn aliased_namespace_selectors_match_bounded_document_nodes() {
    let document = NativeDocument::parse(
            "<main><svg id='svg'><circle id='circle' xlink:href='target'></circle></svg><div id='html'></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("aliased namespace selector fixture document");
    let mut namespaces = BTreeMap::new();
    namespaces.insert("icon".to_owned(), NativeSelectorNamespace::Svg);
    namespaces.insert("link".to_owned(), NativeSelectorNamespace::Xlink);
    let matched_ids = |source: &str| {
        let selector = parse_selector_with_namespaces(source, &namespaces)
            .expect("aliased namespace selector");
        document
            .element_node_ids()
            .filter(|node_id| selector.matches_in_document(&document, *node_id))
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("icon|circle"), vec!["circle"]);
    assert_eq!(matched_ids("[link|href]"), vec!["circle"]);
    assert!(matched_ids("html|circle").is_empty());
}

#[test]
fn selector_parser_supports_bounded_attribute_namespace_selectors() {
    let xlink = parse_selector("[xlink|href='target']").unwrap();
    assert_eq!(
        xlink.compounds[0].attributes[0].namespace,
        NativeAttributeNamespace::Xlink
    );
    assert_eq!(
        parse_selector("[*|href]").unwrap().compounds[0].attributes[0].namespace,
        NativeAttributeNamespace::Any
    );
    assert_eq!(
        parse_selector("[|href]").unwrap().compounds[0].attributes[0].namespace,
        NativeAttributeNamespace::NoNamespace
    );
    assert!(parse_selector("[svg|href]").is_some());
    assert!(parse_selector("[foo|href]").is_none());
    assert!(parse_selector("[xlink|]").is_none());
}

#[test]
fn document_selector_matches_bounded_attribute_operators() {
    let document = NativeDocument::parse(
            "<main><div id='primary' data-token='alpha beta' data-lang='en-US' data-prefix='prefix-value' data-suffix='value-suffix' data-text='middle-value' data-role='button'></div><div id='secondary' data-token='gamma' data-lang='fr'></div><div id='missing'></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("attribute selector fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("[data-token]"), vec!["primary", "secondary"]);
    assert_eq!(matched_ids("[data-token='alpha beta']"), vec!["primary"]);
    assert_eq!(matched_ids("[data-token~=beta]"), vec!["primary"]);
    assert_eq!(matched_ids("[data-lang|=en]"), vec!["primary"]);
    assert_eq!(matched_ids("[data-prefix^=prefix]"), vec!["primary"]);
    assert_eq!(matched_ids("[data-suffix$=suffix]"), vec!["primary"]);
    assert_eq!(matched_ids("[data-text*=middle]"), vec!["primary"]);
    assert!(matched_ids("[data-lang|=e]").is_empty());
}

#[test]
fn document_selector_matches_bounded_namespace_type_selectors() {
    let document = NativeDocument::parse(
            "<main><svg id='svg'><circle id='circle'></circle></svg><div id='html'></div><math><mi id='math'></mi></math></main>",
            &NativeEngineLimits::default(),
        )
        .expect("namespace selector fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("svg|circle"), vec!["circle"]);
    assert_eq!(matched_ids("*|circle"), vec!["circle"]);
    assert_eq!(matched_ids("html|div"), vec!["html"]);
    assert_eq!(matched_ids("math|mi"), vec!["math"]);
    assert_eq!(matched_ids("svg|*"), vec!["svg", "circle"]);
    assert!(matched_ids("|circle").is_empty());
    assert!(matched_ids("html|circle").is_empty());
}

#[test]
fn document_selector_matches_bounded_attribute_namespace_selectors() {
    let document = NativeDocument::parse(
            "<main><svg id='svg' href='plain' xlink:href='target' xml:lang='en'></svg><div id='html' xml:lang='en'></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("attribute namespace selector fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("[xlink|href]"), vec!["svg"]);
    assert_eq!(matched_ids("[xlink|href='target']"), vec!["svg"]);
    assert_eq!(matched_ids("[*|href]"), vec!["svg"]);
    assert_eq!(matched_ids("[|href]"), vec!["svg"]);
    assert_eq!(matched_ids("[xml|lang='en']"), vec!["svg"]);
    assert!(matched_ids("[svg|href]").is_empty());
}

#[test]
fn document_selector_matches_attribute_case_flags() {
    let document = NativeDocument::parse(
            "<main><div id='mixed' data-eq='MiXeD' data-token='Alpha BETA' data-lang='EN-us' data-prefix='PreFix-value' data-suffix='value-SuFfIx' data-text='middle-VALUE'></div><div id='exact' data-eq='mixed'></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("attribute case selector fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("[data-eq='mixed' i]"), vec!["mixed", "exact"]);
    assert_eq!(matched_ids("[data-eq='mixed' s]"), vec!["exact"]);
    assert_eq!(matched_ids("[data-token~='beta' i]"), vec!["mixed"]);
    assert_eq!(matched_ids("[data-lang|='en' i]"), vec!["mixed"]);
    assert_eq!(matched_ids("[data-prefix^='prefix' i]"), vec!["mixed"]);
    assert_eq!(matched_ids("[data-suffix$='suffix' i]"), vec!["mixed"]);
    assert_eq!(matched_ids("[data-text*='middle-value' i]"), vec!["mixed"]);
    assert_eq!(matched_ids("[data-eq='mixed']"), vec!["exact"]);
}

#[test]
fn document_selector_applies_html_default_attribute_value_case_rules() {
    assert_eq!(HTML_CASE_INSENSITIVE_ATTRIBUTE_NAMES.len(), 46);
    let listed_attributes = HTML_CASE_INSENSITIVE_ATTRIBUTE_NAMES
        .iter()
        .map(|name| format!("{name}='MiXeD'"))
        .collect::<Vec<_>>()
        .join(" ");
    let source = format!(
        "<main><div id='listed' {listed_attributes}></div><div id='equals' type='TeXt'></div><div id='includes' type='alpha BETA'></div><div id='dash' type='EN-us'></div><div id='prefix' type='PreFix-value'></div><div id='suffix' type='value-SuFfIx'></div><div id='substring' type='middle-VALUE'></div><div id='editable' contenteditable='TRUE'></div><div id='custom' data-value='MiXeD'></div><div id='unicode' data-value='GrÜn'></div><div id='namespaced' xml:type='TeXt'></div><svg><g id='foreign' type='TeXt'></g></svg></main>"
    );
    let document = NativeDocument::parse(&source, &NativeEngineLimits::default())
        .expect("HTML default attribute case fixture document");
    let matched_ids = |selector: &str| {
        selector_matches_in_document(&document, selector)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    for name in HTML_CASE_INSENSITIVE_ATTRIBUTE_NAMES {
        assert_eq!(
            matched_ids(&format!("[{name}='mixed']")),
            vec!["listed"],
            "HTML default value matching for [{name}=mixed]"
        );
    }
    assert_eq!(matched_ids("[type='text']"), vec!["equals"]);
    assert_eq!(matched_ids("[|type='text']"), vec!["equals"]);
    assert_eq!(matched_ids("[*|type='text']"), vec!["equals"]);
    assert_eq!(matched_ids("[type~='beta']"), vec!["includes"]);
    assert_eq!(matched_ids("[type|='en']"), vec!["dash"]);
    assert_eq!(matched_ids("[type^='prefix']"), vec!["prefix"]);
    assert_eq!(matched_ids("[type$='suffix']"), vec!["suffix"]);
    assert_eq!(matched_ids("[type*='middle']"), vec!["substring"]);
    assert!(matched_ids("[type='text' s]").is_empty());
    assert_eq!(matched_ids("#equals[type='text' i]"), vec!["equals"]);
    assert!(matched_ids("[contenteditable='true']").is_empty());
    assert_eq!(matched_ids("[contenteditable='true' i]"), vec!["editable"]);
    assert!(matched_ids("[data-value='mixed']").is_empty());
    assert_eq!(matched_ids("[data-value='mixed' i]"), vec!["custom"]);
    assert!(matched_ids("[data-value='grün' i]").is_empty());
    assert!(matched_ids("#foreign[type='text']").is_empty());
    assert_eq!(matched_ids("#foreign[type='text' i]"), vec!["foreign"]);
    assert!(matched_ids("#namespaced[xml|type='text']").is_empty());
    assert!(
        matched_ids("#namespaced[xml|type='text' i]").is_empty(),
        "a prefix-looking attribute on an HTML element has no XML namespace"
    );
}

#[test]
fn html_default_attribute_value_case_rules_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "[type='text'] { color: red; } [type='text' s] { background-color: red; } [data-value='mixed'] { color: black; }"
                .into(),
        ])
        .expect("HTML default attribute case stylesheet");
    let document = NativeDocument::parse(
            "<main><div id='mixed' type='TeXt' data-value='MiXeD'></div><div id='exact' type='text' data-value='mixed'></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("HTML default attribute cascade document");

    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };
    assert_eq!(style("mixed").color(), Some(NativeColor::RED));
    assert_eq!(style("mixed").background_color(), None);
    assert_eq!(style("exact").color(), Some(NativeColor::BLACK));
    assert_eq!(style("exact").background_color(), Some(NativeColor::RED));
}

#[test]
fn attribute_selector_operators_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "[data-token] { color: black; } [data-token~='beta'] { color: red; } [data-lang|='en'] { background-color: red; }"
                .into(),
        ])
        .expect("attribute selector stylesheet");
    let document = NativeDocument::parse(
            "<main><div id='primary' data-token='alpha beta' data-lang='en-US'></div><div id='secondary' data-token='gamma' data-lang='fr'></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("attribute selector cascade document");

    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };
    assert_eq!(style("primary").color(), Some(NativeColor::RED));
    assert_eq!(style("primary").background_color(), Some(NativeColor::RED));
    assert_eq!(style("secondary").color(), Some(NativeColor::BLACK));
    assert_eq!(style("secondary").background_color(), None);
}

#[test]
fn attribute_selector_case_flags_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "[data-role='button' i] { color: red; } [data-role='button' s] { background-color: red; }"
            .into(),
    ])
    .expect("attribute case selector stylesheet");
    let document = NativeDocument::parse(
            "<main><button id='mixed' data-role='BuTtOn'></button><button id='exact' data-role='button'></button></main>",
            &NativeEngineLimits::default(),
        )
        .expect("attribute case cascade document");

    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };
    assert_eq!(style("mixed").color(), Some(NativeColor::RED));
    assert_eq!(style("mixed").background_color(), None);
    assert_eq!(style("exact").color(), Some(NativeColor::RED));
    assert_eq!(style("exact").background_color(), Some(NativeColor::RED));
}

#[test]
fn namespace_type_selectors_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "svg|circle { color: red; } html|div { background-color: red; }".into(),
    ])
    .expect("namespace selector stylesheet");
    let document = NativeDocument::parse(
        "<main><svg><circle id='circle'></circle></svg><div id='html'></div></main>",
        &NativeEngineLimits::default(),
    )
    .expect("namespace cascade document");

    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };
    assert_eq!(style("circle").color(), Some(NativeColor::RED));
    assert_eq!(style("circle").background_color(), None);
    assert_eq!(style("html").color(), None);
    assert_eq!(style("html").background_color(), Some(NativeColor::RED));
}

#[test]
fn attribute_namespace_selectors_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "[xlink|href] { color: red; } [|href] { background-color: red; }".into(),
    ])
    .expect("attribute namespace stylesheet");
    let document = NativeDocument::parse(
        "<main><svg id='svg' href='plain' xlink:href='target'></svg><div id='html'></div></main>",
        &NativeEngineLimits::default(),
    )
    .expect("attribute namespace cascade document");

    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };
    assert_eq!(style("svg").color(), Some(NativeColor::RED));
    assert_eq!(style("svg").background_color(), Some(NativeColor::RED));
    assert_eq!(style("html").color(), None);
    assert_eq!(style("html").background_color(), None);
}

#[test]
fn namespace_aliases_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@namespace icon url(http://www.w3.org/2000/svg); @namespace link url(http://www.w3.org/1999/xlink); @namespace url(http://www.w3.org/2000/svg); icon|circle { color: red; } :is(icon|circle) { color: blue; } circle { background-color: red; } [link|href] { background-color: black; } html|div { color: green; }"
                .into(),
        ])
        .expect("namespace alias stylesheet");
    let document = NativeDocument::parse(
            "<main><svg><circle id='circle' xlink:href='target'></circle></svg><div id='html'></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("namespace alias cascade document");

    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };
    assert_eq!(
        style("circle").color(),
        Some(NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        })
    );
    assert_eq!(style("circle").background_color(), Some(NativeColor::BLACK));
    assert_eq!(
        style("html").color(),
        Some(NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        })
    );
    assert_eq!(style("html").background_color(), None);
}

#[test]
fn document_selector_matches_bounded_child_combinators() {
    let document = NativeDocument::parse(
            "<main><section><button id='first'></button><button id='second'></button><div><button id='deep'></button></div><button id='last'></button></section></main>",
            &NativeEngineLimits::default(),
        )
        .expect("child combinator fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(
        matched_ids("main > section > button"),
        vec!["first", "second", "last"]
    );
    assert_eq!(matched_ids("section > div > button"), vec!["deep"]);
    assert_eq!(
        matched_ids("main section button"),
        vec!["first", "second", "deep", "last"]
    );
    let stylesheet = NativeStylesheet::from_sources(vec![
        "main > section > button { color: red; } #first + button { color: blue; }".into(),
    ])
    .expect("combinator stylesheet");
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };
    assert_eq!(
        stylesheet
            .computed_for_in_document(
                &document,
                document.resolve_target("id=second").unwrap(),
                None,
            )
            .color(),
        Some(blue)
    );
    assert_eq!(
        stylesheet
            .computed_for_in_document(&document, document.resolve_target("id=last").unwrap(), None,)
            .color(),
        Some(NativeColor::RED)
    );
}

#[test]
fn document_selector_matches_bounded_sibling_combinators() {
    let document = NativeDocument::parse(
            "<main><button id='first'></button><button id='second'></button><div id='middle'></div><button id='last'></button></main>",
            &NativeEngineLimits::default(),
        )
        .expect("sibling combinator fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("#first + button"), vec!["second"]);
    assert_eq!(matched_ids("#first ~ button"), vec!["second", "last"]);
}

#[test]
fn selector_parser_supports_bounded_pseudo_classes() {
    let selector = parse_selector("button:first-child:checked").unwrap();
    assert_eq!(selector.specificity, 21);
    assert!(parse_selector(":empty").is_some());
    assert!(parse_selector("button:first-of-type").is_some());
    assert!(parse_selector("button:last-of-type").is_some());
    assert!(parse_selector("button:only-of-type").is_some());
    assert_eq!(
        parse_selector("button:not(.active)").unwrap().specificity,
        11
    );
    assert_eq!(
        parse_selector("button:is(.active, #primary)")
            .unwrap()
            .specificity,
        101
    );
    assert_eq!(
        parse_selector("button:where(.active, #primary)")
            .unwrap()
            .specificity,
        1
    );
    assert!(parse_selector("button:hover").is_some());
    assert_eq!(
        parse_selector("button:nth-child(2)").unwrap().specificity,
        11
    );
    assert_eq!(
        parse_selector("button:nth-child(2n+1)")
            .unwrap()
            .specificity,
        11
    );
    assert!(parse_selector("button:nth-child(odd)").is_some());
    assert!(parse_selector("button:nth-child(2n+)").is_none());
    assert_eq!(
        parse_selector("button:nth-child(2 of .item)")
            .unwrap()
            .specificity,
        21
    );
    assert_eq!(
        parse_selector("button:nth-child(2 of .item, #primary)")
            .unwrap()
            .specificity,
        111
    );
    assert!(parse_selector("button:nth-of-type(2 of .item)").is_none());
    assert!(parse_selector("button:nth-child(2 of)").is_none());
    assert!(parse_selector("button:nth-child(2 of [data-label='of'])").is_some());
    assert_eq!(
        parse_selector("button:has(.child)").unwrap().specificity,
        11
    );
    assert_eq!(
        parse_selector("button:has(> .child)").unwrap().specificity,
        11
    );
    assert!(parse_selector("button:has(>)").is_none());
    assert!(parse_selector("button:not(.active").is_none());
    assert_eq!(parse_selector(":lang(en)").unwrap().specificity, 10);
    assert!(parse_selector(":lang('en-US')").is_some());
    assert!(parse_selector(":lang(en-*)").is_some());
    assert!(parse_selector(":lang(en us)").is_none());
    assert!(parse_selector(":lang(en_*)").is_none());
    assert_eq!(parse_selector(":dir(rtl)").unwrap().specificity, 10);
    assert!(parse_selector(":dir(LTR)").is_some());
    assert!(parse_selector(":dir(auto)").is_none());
    assert!(parse_selector(":dir('rtl')").is_none());
    assert!(parse_selector("button::before").is_none());
    for source in [
        "button:focus",
        "button:focus-within",
        "button:hover",
        "button:focus-visible",
        "input:valid",
        "input:invalid",
        "input:read-only",
        "input:read-write",
        "input:placeholder-shown",
        "input:indeterminate",
        "input:default",
        "input:in-range",
        "input:out-of-range",
        "div:scope",
        "div:target",
    ] {
        assert_eq!(parse_selector(source).unwrap().specificity, 11);
    }
}

#[test]
fn document_selector_matches_bounded_structural_pseudo_classes() {
    let mut document = NativeDocument::parse(
            "<main id='root'><div id='first'></div><section id='middle'><span id='only'></span></section><div id='empty'></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("pseudo-class fixture document");
    document
        .set_css_target_from_url("fixture://example.test/page#middle")
        .expect("target fragment URL");

    let matched_id = |source: &str| {
        let matches = selector_matches_in_document(&document, source).unwrap();
        assert_eq!(matches.len(), 1, "selector {source:?}");
        document
            .node(matches[0])
            .and_then(|node| node.attribute("id"))
    };
    assert_eq!(matched_id("#first:first-child"), Some("first"));
    assert_eq!(matched_id("#empty:last-child"), Some("empty"));
    assert_eq!(matched_id("#only:only-child"), Some("only"));
    assert_eq!(matched_id("#empty:empty"), Some("empty"));
    assert_eq!(matched_id("#first:first-of-type"), Some("first"));
    assert_eq!(matched_id("#empty:last-of-type"), Some("empty"));
    assert_eq!(matched_id("#middle:only-of-type"), Some("middle"));
    assert_eq!(matched_id("#only:only-of-type"), Some("only"));

    let root_element = document
        .node(document.root())
        .expect("document root")
        .children()
        .iter()
        .copied()
        .find(|node_id| {
            document
                .node(*node_id)
                .is_some_and(|node| node.element_name().is_some())
        })
        .expect("root element");
    let root_name = document
        .node(root_element)
        .and_then(|node| node.element_name())
        .expect("root element name")
        .to_owned();

    assert_eq!(
        selector_matches_in_document(&document, &format!("{root_name}:root")).unwrap(),
        vec![root_element]
    );
    assert_eq!(matched_id("#middle:target"), Some("middle"));
    assert!(
        selector_matches_in_document(&document, "#empty:target")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn target_pseudo_class_applies_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "#middle:target { background-color: black; } #empty:target { background-color: red; }"
            .into(),
    ])
    .expect("target pseudo-class stylesheet");
    let mut document = NativeDocument::parse(
        "<main><section id='middle'></section><div id='empty'></div></main>",
        &NativeEngineLimits::default(),
    )
    .expect("target pseudo-class cascade document");
    document
        .set_css_target_from_url("fixture://example.test/page#middle")
        .expect("middle target fragment");
    let middle = document.resolve_target("id=middle").unwrap();
    let empty = document.resolve_target("id=empty").unwrap();
    assert_eq!(
        stylesheet
            .computed_for_in_document(&document, middle, None)
            .background_color(),
        Some(NativeColor::BLACK)
    );
    assert_eq!(
        stylesheet
            .computed_for_in_document(&document, empty, None)
            .background_color(),
        None
    );

    document
        .set_css_target_from_url("fixture://example.test/page#empty")
        .expect("empty target fragment");
    assert_eq!(
        stylesheet
            .computed_for_in_document(&document, empty, None)
            .background_color(),
        Some(NativeColor::RED)
    );
}

#[test]
fn document_selector_matches_bounded_form_pseudo_classes() {
    let mut document = NativeDocument::parse(
            "<form id='form'><input id='valid' value='ok'><input id='invalid' required placeholder='Required'><input id='focused' placeholder='Name'><input id='readonly' readonly value='locked'><input id='placeholder' placeholder='Name'><input id='checked' type='checkbox' checked><input id='low' type='number' value='2' min='3'><input id='mid' type='number' value='5' min='3' max='7'><input id='high' type='number' value='9' min='3' max='7'><input id='unbounded' type='number' value='5'><select id='select'><option id='default-option' selected>Choice</option></select><div id='editable' contenteditable='true'></div><div id='plain'></div></form>",
            &NativeEngineLimits::default(),
        )
        .expect("form pseudo-class fixture document");
    let focused = document.resolve_target("id=focused").unwrap();
    document.apply_script_focus(focused).unwrap();
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .filter_map(|node_id| document.node(node_id)?.attribute("id"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("#focused:focus"), vec!["focused"]);
    assert_eq!(matched_ids("#focused:focus-visible"), vec!["focused"]);
    assert_eq!(matched_ids("form:focus-within"), vec!["form"]);
    assert_eq!(matched_ids("#valid:valid"), vec!["valid"]);
    assert_eq!(matched_ids("#invalid:invalid"), vec!["invalid"]);
    assert_eq!(matched_ids("#readonly:read-only"), vec!["readonly"]);
    assert_eq!(matched_ids("#valid:read-write"), vec!["valid"]);
    assert_eq!(
        matched_ids("#placeholder:placeholder-shown"),
        vec!["placeholder"]
    );
    assert_eq!(matched_ids("#checked:default"), vec!["checked"]);
    assert_eq!(
        matched_ids("#default-option:default"),
        vec!["default-option"]
    );
    assert_eq!(matched_ids("#mid:in-range"), vec!["mid"]);
    assert_eq!(matched_ids("#low:out-of-range"), vec!["low"]);
    assert_eq!(matched_ids("#high:out-of-range"), vec!["high"]);
    assert!(matched_ids("#unbounded:in-range").is_empty());
    assert_eq!(matched_ids("#editable:read-write"), vec!["editable"]);
    assert_eq!(matched_ids("#plain:read-only"), vec!["plain"]);
}

#[test]
fn form_pseudo_classes_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "input:valid { color: black; } input:invalid { color: red; } input:focus { color: red; } form:focus-within { background-color: red; } input:read-only { background-color: black; } input:placeholder-shown { background-color: red; } input:in-range { background-color: black; } input:out-of-range { background-color: red; } input:default { color: red; } option:default { color: black; }"
                .into(),
        ])
        .expect("form pseudo-class stylesheet");
    let mut document = NativeDocument::parse(
            "<form id='form'><input id='valid' value='ok'><input id='invalid' required><input id='focused'><input id='readonly' readonly value='locked'><input id='placeholder' placeholder='Name'><input id='checked' type='checkbox' checked><input id='low' type='number' value='2' min='3'><input id='mid' type='number' value='5' min='3' max='7'><input id='high' type='number' value='9' min='3' max='7'><select><option id='default-option' selected>Choice</option></select></form>",
            &NativeEngineLimits::default(),
        )
        .expect("form pseudo-class cascade document");
    let focused = document.resolve_target("id=focused").unwrap();
    document.apply_script_focus(focused).unwrap();
    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };

    assert_eq!(style("valid").color(), Some(NativeColor::BLACK));
    assert_eq!(style("invalid").color(), Some(NativeColor::RED));
    assert_eq!(style("focused").color(), Some(NativeColor::RED));
    assert_eq!(style("form").background_color(), Some(NativeColor::RED));
    assert_eq!(
        style("readonly").background_color(),
        Some(NativeColor::BLACK)
    );
    assert_eq!(
        style("placeholder").background_color(),
        Some(NativeColor::RED)
    );
    assert_eq!(style("mid").background_color(), Some(NativeColor::BLACK));
    assert_eq!(style("low").background_color(), Some(NativeColor::RED));
    assert_eq!(style("high").background_color(), Some(NativeColor::RED));
    assert_eq!(style("checked").color(), Some(NativeColor::RED));
    assert_eq!(style("default-option").color(), Some(NativeColor::BLACK));
}
#[test]
fn document_selector_matches_bounded_language_pseudo_class() {
    let document = NativeDocument::parse(
            "<main id='english' lang='en'><section id='inherit'><p id='french' lang='fr-FR'><span id='french-child'></span></p><p id='german' lang='de-DE'></p><p id='plain'></p></section></main>",
            &NativeEngineLimits::default(),
        )
        .expect("language pseudo-class fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .filter_map(|node_id| document.node(node_id)?.attribute("id"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        matched_ids(":lang(en)"),
        vec!["english", "inherit", "plain"]
    );
    assert_eq!(
        matched_ids(":lang(en-*)"),
        vec!["english", "inherit", "plain"]
    );
    assert_eq!(matched_ids(":lang(fr)"), vec!["french", "french-child"]);
    assert_eq!(matched_ids(":lang(de)"), vec!["german"]);
    assert!(matched_ids(":lang(es)").is_empty());
}

#[test]
fn language_pseudo_class_applies_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            ":lang(en) { background-color: red; } :lang(fr) { background-color: black; } :lang(de) { background-color: green; }"
                .into(),
        ])
        .expect("language pseudo-class stylesheet");
    let document = NativeDocument::parse(
            "<main lang='en'><p id='plain'></p><p id='french' lang='fr-FR'><span id='french-child'></span></p><p id='german' lang='de-DE'></p></main>",
            &NativeEngineLimits::default(),
        )
        .expect("language pseudo-class cascade document");
    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };

    assert_eq!(style("plain").background_color(), Some(NativeColor::RED));
    assert_eq!(style("french").background_color(), Some(NativeColor::BLACK));
    assert_eq!(
        style("french-child").background_color(),
        Some(NativeColor::BLACK)
    );
    assert_eq!(
        style("german").background_color(),
        Some(NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        })
    );
}

#[test]
fn document_selector_matches_bounded_direction_pseudo_class() {
    let document = NativeDocument::parse(
            "<main id='default'><section id='rtl' dir='rtl'><p id='rtl-child'></p><p id='ltr' dir='ltr'><span id='ltr-child'></span></p><p id='auto' dir='auto'></p></section><p id='last'></p></main>",
            &NativeEngineLimits::default(),
        )
        .expect("direction pseudo-class fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .filter_map(|node_id| document.node(node_id)?.attribute("id"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        matched_ids(":dir(ltr)"),
        vec!["default", "ltr", "ltr-child", "last"]
    );
    assert_eq!(matched_ids(":dir(rtl)"), vec!["rtl", "rtl-child"]);
}

#[test]
fn direction_pseudo_class_applies_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        ":dir(ltr) { background-color: red; } :dir(rtl) { background-color: black; }".into(),
    ])
    .expect("direction pseudo-class stylesheet");
    let document = NativeDocument::parse(
            "<main id='default'><section id='rtl' dir='rtl'><p id='rtl-child'></p><p id='ltr' dir='ltr'><span id='ltr-child'></span></p><p id='auto' dir='auto'></p></section></main>",
            &NativeEngineLimits::default(),
        )
        .expect("direction pseudo-class cascade document");
    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };

    assert_eq!(style("default").background_color(), Some(NativeColor::RED));
    assert_eq!(style("rtl").background_color(), Some(NativeColor::BLACK));
    assert_eq!(
        style("rtl-child").background_color(),
        Some(NativeColor::BLACK)
    );
    assert_eq!(style("ltr").background_color(), Some(NativeColor::RED));
    assert_eq!(
        style("ltr-child").background_color(),
        Some(NativeColor::RED)
    );
    assert_eq!(style("auto").background_color(), None);
}
#[test]
fn document_selector_matches_bounded_state_pseudo_classes() {
    let document = NativeDocument::parse(
            "<form><input id='checked' type='checkbox' checked><input id='disabled' disabled><input id='normal'><input id='required' required><a id='link' href='/target'>Link</a></form>",
            &NativeEngineLimits::default(),
        )
        .expect("state pseudo-class fixture document");

    let matched_id = |source: &str| {
        let matches = selector_matches_in_document(&document, source).unwrap();
        assert_eq!(matches.len(), 1, "selector {source:?}");
        document
            .node(matches[0])
            .and_then(|node| node.attribute("id"))
    };
    assert_eq!(matched_id("#checked:checked"), Some("checked"));
    assert_eq!(matched_id("#disabled:disabled"), Some("disabled"));
    assert_eq!(matched_id("#normal:enabled"), Some("normal"));
    assert_eq!(matched_id("#required:required"), Some("required"));
    assert_eq!(matched_id("#normal:optional"), Some("normal"));
    assert_eq!(matched_id("a:link"), Some("link"));
    assert_eq!(matched_id("a:any-link"), Some("link"));
    assert!(
        selector_matches_in_document(&document, "#normal:nth-child(1)")
            .unwrap()
            .is_empty()
    );
    assert!(
        selector_matches_in_document(&document, "#normal:not(input)")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn document_selector_matches_bounded_user_validation_state() {
    let mut document = NativeDocument::parse(
        "<form><input id='required' required><input id='optional'></form>",
        &NativeEngineLimits::default(),
    )
    .expect("user validation pseudo-class fixture document");
    let required = document.resolve_target("id=required").unwrap();
    let optional = document.resolve_target("id=optional").unwrap();
    assert!(
        selector_matches_in_document(&document, "#required:user-invalid")
            .unwrap()
            .is_empty()
    );
    assert!(
        selector_matches_in_document(&document, "#required:user-valid")
            .unwrap()
            .is_empty()
    );

    document.apply_click(required).unwrap();
    assert_eq!(
        selector_matches_in_document(&document, "#required:user-invalid").unwrap(),
        vec![required]
    );
    document.apply_type(required, "value").unwrap();
    assert_eq!(
        selector_matches_in_document(&document, "#required:user-valid").unwrap(),
        vec![required]
    );

    document.apply_click(optional).unwrap();
    assert_eq!(
        selector_matches_in_document(&document, "#optional:user-valid").unwrap(),
        vec![optional]
    );
}

#[test]
fn user_validation_pseudo_classes_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#required:user-invalid { background-color: red; } #required:user-valid { background-color: green; }"
                .into(),
        ])
        .expect("user validation pseudo-class stylesheet");
    let mut document = NativeDocument::parse(
        "<form><input id='required' required></form>",
        &NativeEngineLimits::default(),
    )
    .expect("user validation pseudo-class cascade document");
    let required = document.resolve_target("id=required").unwrap();
    assert_eq!(
        stylesheet
            .computed_for_in_document(&document, required, None)
            .background_color(),
        None
    );
    document.apply_click(required).unwrap();
    assert_eq!(
        stylesheet
            .computed_for_in_document(&document, required, None)
            .background_color(),
        Some(NativeColor::RED)
    );
    document.apply_type(required, "value").unwrap();
    assert_eq!(
        stylesheet
            .computed_for_in_document(&document, required, None)
            .background_color(),
        Some(NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        })
    );
}

#[test]
fn document_selector_matches_bounded_scope_pseudo_class() {
    let document = NativeDocument::parse(
            "<html><head></head><body><main id='main'><button id='button'>Button</button></main></body></html>",
            &NativeEngineLimits::default(),
        )
        .expect("scope pseudo-class fixture document");

    let scope = selector_matches_in_document(&document, ":scope").unwrap();
    assert_eq!(scope.len(), 1);
    assert_eq!(
        document.node(scope[0]).and_then(|node| node.element_name()),
        Some("html")
    );
    let body = selector_matches_in_document(&document, ":scope > body").unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(
        document.node(body[0]).and_then(|node| node.element_name()),
        Some("body")
    );
    assert!(
        selector_matches_in_document(&document, "body:scope")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn document_selector_matches_bounded_hover_state() {
    let mut document = NativeDocument::parse(
            "<main id='main'><section id='first'><button id='first-target'>First</button></section><section id='second'><button id='second-target'>Second</button></section></main>",
            &NativeEngineLimits::default(),
        )
        .expect("hover pseudo-class fixture document");
    fn matched_ids(document: &NativeDocument, source: &str) -> Vec<String> {
        selector_matches_in_document(document, source)
            .unwrap()
            .into_iter()
            .filter_map(|node_id| document.node(node_id)?.attribute("id"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    }

    let first_target = document.resolve_target("id=first-target").unwrap();
    document.apply_hover(first_target).unwrap();
    assert_eq!(
        matched_ids(&document, ":hover"),
        vec!["main", "first", "first-target"]
    );
    assert_eq!(
        matched_ids(&document, "#second-target:hover"),
        Vec::<String>::new()
    );

    let second_target = document.resolve_target("id=second-target").unwrap();
    document.apply_hover(second_target).unwrap();
    assert_eq!(
        matched_ids(&document, ":hover"),
        vec!["main", "second", "second-target"]
    );
    assert!(matched_ids(&document, "#first:hover").is_empty());
}

#[test]
fn document_selector_matches_bounded_indeterminate_pseudo_class() {
    let document = NativeDocument::parse(
            "<form><input id='empty-a' type='radio' name='empty'><input id='empty-b' type='radio' name='empty'><input id='checked-radio' type='radio' name='checked' checked><input id='unchecked-radio' type='radio' name='checked'><progress id='pending'></progress><progress id='complete' value='1' max='2'></progress><progress id='invalid' value='bogus'></progress><input id='checkbox' type='checkbox'></form>",
            &NativeEngineLimits::default(),
        )
        .expect("indeterminate pseudo-class fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .filter_map(|node_id| document.node(node_id)?.attribute("id"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("#empty-a:indeterminate"), vec!["empty-a"]);
    assert_eq!(matched_ids("#empty-b:indeterminate"), vec!["empty-b"]);
    assert!(matched_ids("#checked-radio:indeterminate").is_empty());
    assert!(matched_ids("#unchecked-radio:indeterminate").is_empty());
    assert_eq!(matched_ids("#pending:indeterminate"), vec!["pending"]);
    assert!(matched_ids("#complete:indeterminate").is_empty());
    assert_eq!(matched_ids("#invalid:indeterminate"), vec!["invalid"]);
    assert!(matched_ids("#checkbox:indeterminate").is_empty());
}

#[test]
fn indeterminate_pseudo_class_applies_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "input:indeterminate, progress:indeterminate { background-color: black; }".into(),
    ])
    .expect("indeterminate pseudo-class stylesheet");
    let document = NativeDocument::parse(
            "<form><input id='radio' type='radio' name='group'><progress id='progress'></progress><progress id='complete' value='1'></progress></form>",
            &NativeEngineLimits::default(),
        )
        .expect("indeterminate pseudo-class cascade document");
    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };

    assert_eq!(style("radio").background_color(), Some(NativeColor::BLACK));
    assert_eq!(
        style("progress").background_color(),
        Some(NativeColor::BLACK)
    );
    assert_eq!(style("complete").background_color(), None);
}
#[test]
fn scope_pseudo_class_applies_during_style_cascade() {
    let stylesheet =
        NativeStylesheet::from_sources(vec![":scope > body { background-color: black; }".into()])
            .expect("scope pseudo-class stylesheet");
    let document = NativeDocument::parse(
        "<html><head></head><body><main id='main'>Main</main></body></html>",
        &NativeEngineLimits::default(),
    )
    .expect("scope pseudo-class cascade document");
    let main = document.resolve_target("id=main").unwrap();
    let body = document
        .node(main)
        .and_then(|node| node.parent())
        .expect("main parent body");
    let body_style = stylesheet.computed_for_in_document(&document, body, None);
    let main_style = stylesheet.computed_for_in_document(&document, main, None);

    assert_eq!(body_style.background_color(), Some(NativeColor::BLACK));
    assert_eq!(main_style.background_color(), None);
}

#[test]
fn hover_pseudo_class_applies_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "section:hover { color: red; } button:hover { background-color: black; }".into(),
    ])
    .expect("hover pseudo-class stylesheet");
    let mut document = NativeDocument::parse(
            "<main><section id='section'><button id='hovered'>Hovered</button></section><button id='plain'>Plain</button></main>",
            &NativeEngineLimits::default(),
        )
        .expect("hover pseudo-class cascade document");
    let hovered = document.resolve_target("id=hovered").unwrap();
    document.apply_hover(hovered).unwrap();
    let section_style = stylesheet.computed_for_in_document(
        &document,
        document.resolve_target("id=section").unwrap(),
        None,
    );
    let hovered_style = stylesheet.computed_for_in_document(&document, hovered, None);
    let plain_style = stylesheet.computed_for_in_document(
        &document,
        document.resolve_target("id=plain").unwrap(),
        None,
    );

    assert_eq!(section_style.color(), Some(NativeColor::RED));
    assert_eq!(hovered_style.background_color(), Some(NativeColor::BLACK));
    assert_eq!(plain_style.color(), None);
    assert_eq!(plain_style.background_color(), None);
}

#[test]
fn document_selector_matches_bounded_nth_pseudo_classes() {
    let document = NativeDocument::parse(
            "<ul id='items'><li id='one' data-kind='pick'></li><li id='two'></li><span id='marker'></span><li id='three' data-kind='pick'></li><li id='four' data-kind='pick'></li></ul>",
            &NativeEngineLimits::default(),
        )
        .expect("nth pseudo-class fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("li:nth-child(2)"), vec!["two"]);
    assert_eq!(matched_ids("li:nth-last-child(1)"), vec!["four"]);
    assert_eq!(matched_ids("li:nth-child(2n+1)"), vec!["one", "four"]);
    assert_eq!(matched_ids("li:nth-child(-n+2)"), vec!["one", "two"]);
    assert_eq!(matched_ids("li:nth-of-type(2)"), vec!["two"]);
    assert_eq!(matched_ids("li:nth-last-of-type(1)"), vec!["four"]);
    assert_eq!(matched_ids("li:nth-of-type(odd)"), vec!["one", "three"]);
    assert_eq!(matched_ids("li:nth-child(2 of li)"), vec!["two"]);
    assert_eq!(matched_ids("li:nth-last-child(1 of li)"), vec!["four"]);
    assert_eq!(
        matched_ids("li:nth-child(2 of li:not(#two))"),
        vec!["three"]
    );
    assert_eq!(
        matched_ids("li:nth-child(2 of [data-kind=pick])"),
        vec!["three"]
    );
}

#[test]
fn nth_pseudo_classes_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "li { color: black; } li:nth-child(2n+1) { color: red; } li:nth-child(2 of li[data-kind=pick]) { color: blue; } li:nth-child(2 of li) { background-color: red; } li:nth-last-of-type(1) { background-color: red; }"
                .into(),
        ])
        .expect("nth pseudo-class stylesheet");
    let document = NativeDocument::parse(
            "<ul><li id='one' data-kind='pick'></li><li id='two'></li><span></span><li id='three' data-kind='pick'></li><li id='four' data-kind='pick'></li></ul>",
            &NativeEngineLimits::default(),
        )
        .expect("nth pseudo-class cascade document");

    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };
    assert_eq!(style("one").color(), Some(NativeColor::RED));
    assert_eq!(style("two").color(), Some(NativeColor::BLACK));
    assert_eq!(style("four").color(), Some(NativeColor::RED));
    assert_eq!(style("four").background_color(), Some(NativeColor::RED));
    assert_eq!(style("two").background_color(), Some(NativeColor::RED));
    assert_eq!(
        style("three").color(),
        Some(NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        })
    );
    assert_eq!(style("three").background_color(), None);
}

#[test]
fn document_selector_matches_bounded_negation_pseudo_class() {
    let document = NativeDocument::parse(
            "<main><button id='active' class='active'></button><button id='plain'></button><div id='other'></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("negation pseudo-class fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("button:not(.active)"), vec!["plain"]);
    assert_eq!(matched_ids("#plain:not(.missing)"), vec!["plain"]);
    assert!(matched_ids("#active:not(.active)").is_empty());
    assert!(matched_ids("#plain:not(button)").is_empty());
}

#[test]
fn document_selector_matches_bounded_selector_list_pseudos() {
    let document = NativeDocument::parse(
            "<main><button id='primary' class='primary'></button><button id='secondary' class='secondary'></button><span id='plain'></span></main>",
            &NativeEngineLimits::default(),
        )
        .expect("selector-list pseudo-class fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(
        matched_ids(":is(button.primary, button.secondary)"),
        vec!["primary", "secondary"]
    );
    assert_eq!(
        matched_ids(":where(.primary, .secondary)"),
        vec!["primary", "secondary"]
    );
    assert_eq!(
        matched_ids("#primary:not(:is(.secondary, .other))"),
        vec!["primary"]
    );
}

#[test]
fn document_selector_matches_bounded_has_pseudo_class() {
    let document = NativeDocument::parse(
            "<main id='root'><section id='parent'><span id='child' class='target'></span><span id='tail' class='tail'></span></section><section id='empty'></section><div id='anchor'></div><span id='next' class='next'></span><span id='later' class='later'></span></main>",
            &NativeEngineLimits::default(),
        )
        .expect("has pseudo-class fixture document");
    let matched_ids = |source: &str| {
        selector_matches_in_document(&document, source)
            .unwrap()
            .into_iter()
            .map(|node_id| {
                document
                    .node(node_id)
                    .and_then(|node| node.attribute("id"))
                    .expect("matched element id")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(matched_ids("section:has(.target)"), vec!["parent"]);
    assert_eq!(matched_ids("#parent:has(> .target)"), vec!["parent"]);
    assert!(matched_ids("#parent:has(section .target)").is_empty());
    assert_eq!(matched_ids("main:has(section > .target)"), vec!["root"]);
    assert_eq!(matched_ids("#anchor:has(+ .next)"), vec!["anchor"]);
    assert_eq!(matched_ids("#anchor:has(~ .later)"), vec!["anchor"]);
    assert!(matched_ids("#empty:has(.target)").is_empty());
}

#[test]
fn has_pseudo_class_applies_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        ".card { color: black; } .card:has(.selected) { color: red; }".into(),
    ])
    .expect("has pseudo-class stylesheet");
    let document = NativeDocument::parse(
            "<main><article id='selected-card' class='card'><span class='selected'></span></article><article id='plain-card' class='card'></article></main>",
            &NativeEngineLimits::default(),
        )
        .expect("has pseudo-class cascade document");

    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };
    assert_eq!(style("selected-card").color(), Some(NativeColor::RED));
    assert_eq!(style("plain-card").color(), Some(NativeColor::BLACK));
}

#[test]
fn functional_pseudo_classes_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            ".item { color: black; } .item:is(.primary, .secondary) { color: red; } .item:not(.secondary) { background-color: red; }"
                .into(),
        ])
        .expect("functional pseudo-class stylesheet");
    let document = NativeDocument::parse(
            "<main><button id='primary' class='item primary'></button><button id='secondary' class='item secondary'></button><button id='plain' class='item'></button></main>",
            &NativeEngineLimits::default(),
        )
        .expect("functional pseudo-class cascade document");

    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };
    assert_eq!(style("primary").color(), Some(NativeColor::RED));
    assert_eq!(style("secondary").color(), Some(NativeColor::RED));
    assert_eq!(style("plain").color(), Some(NativeColor::BLACK));
    assert_eq!(style("primary").background_color(), Some(NativeColor::RED));
    assert_eq!(style("secondary").background_color(), None);
}

#[test]
fn of_type_pseudo_classes_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "div:first-of-type { color: red; } div:last-of-type { background-color: red; } span:only-of-type { color: red; }"
                .into(),
        ])
        .expect("of-type pseudo-class stylesheet");
    let document = NativeDocument::parse(
        "<main><div id='first'></div><span id='only'></span><div id='last'></div></main>",
        &NativeEngineLimits::default(),
    )
    .expect("of-type pseudo-class cascade document");

    let style = |id: &str| {
        stylesheet.computed_for_in_document(
            &document,
            document.resolve_target(&format!("id={id}")).unwrap(),
            None,
        )
    };
    assert_eq!(style("first").color(), Some(NativeColor::RED));
    assert_eq!(style("only").color(), Some(NativeColor::RED));
    assert_eq!(style("last").background_color(), Some(NativeColor::RED));
}

#[test]
fn document_pseudo_classes_apply_during_style_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#first:first-child { color: red; } #empty:last-child { color: blue; } #only:only-child { color: black; }"
                .into(),
        ])
        .expect("pseudo-class stylesheet");
    let document = NativeDocument::parse(
            "<main><div id='first'></div><section><span id='only'></span></section><div id='empty'></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("pseudo-class cascade document");

    assert_eq!(
        stylesheet
            .computed_for_in_document(
                &document,
                document.resolve_target("id=first").unwrap(),
                None
            )
            .color(),
        Some(NativeColor::RED)
    );
    assert_eq!(
        stylesheet
            .computed_for_in_document(
                &document,
                document.resolve_target("id=empty").unwrap(),
                None
            )
            .color(),
        Some(NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        })
    );
    assert_eq!(
        stylesheet
            .computed_for_in_document(&document, document.resolve_target("id=only").unwrap(), None)
            .color(),
        Some(NativeColor::BLACK)
    );
}

#[test]
fn descendant_selector_matches_owned_ancestors_before_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "main .card button { display: none; background-color: red; } main button { display: block; }"
                .into(),
        ])
        .unwrap();
    let document = NativeDocument::parse(
            "<main><section class='card'><button id='target'>Target</button></section><button id='other'>Other</button></main>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let target = document.resolve_target("id=target").unwrap();
    let other = document.resolve_target("id=other").unwrap();

    let target_style = stylesheet.computed_for_in_document(&document, target, None);
    assert!(target_style.hidden());
    assert_eq!(target_style.background_color(), Some(NativeColor::RED));

    let other_style = stylesheet.computed_for_in_document(&document, other, None);
    assert!(!other_style.hidden());
    assert_eq!(other_style.background_color(), None);
}

#[test]
fn native_rule_limit_returns_a_typed_error_for_larger_stylesheets() {
    let source_for = |count: usize| {
        (0..count)
            .map(|index| format!(".rule-{index} {{ color: red; }}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let stylesheet =
        NativeStylesheet::from_sources(vec![source_for(MAX_NATIVE_STYLE_RULES)]).unwrap();
    assert_eq!(stylesheet.rules.len(), MAX_NATIVE_STYLE_RULES);

    let error = NativeStylesheet::from_sources(vec![source_for(MAX_NATIVE_STYLE_RULES + 1)])
        .expect_err("the native selector matcher must keep its bounded work budget");
    assert!(
        error
            .to_string()
            .contains("CSS style rules exceeds limit 4096 (actual 4097)")
    );
}

#[test]
fn native_four_thousand_rule_sheet_remains_bounded_and_computable() {
    let parse_started = std::time::Instant::now();
    let source = (0..MAX_NATIVE_STYLE_RULES)
        .map(|index| format!(".never-match-{index} {{ color: red; }}"))
        .collect::<Vec<_>>()
        .join("\n");
    let stylesheet = NativeStylesheet::from_sources(vec![source]).unwrap();
    let parse_elapsed = parse_started.elapsed();
    let html = format!("<main>{}</main>", "<div>fixture</div>".repeat(128));
    let document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
    let elements = document.element_node_ids().collect::<Vec<_>>();
    assert_eq!(elements.len(), 132);
    assert!(
        stylesheet
            .rule_index
            .candidates(document.node(elements[0]).unwrap())
            .is_empty()
    );
    let baseline = NativeStylesheet::default();
    let style_started = std::time::Instant::now();
    for node in elements {
        let actual = stylesheet.computed_for_in_document(&document, node, None);
        let expected = baseline.computed_for_in_document(&document, node, None);
        assert!(
            actual == expected,
            "unmatched class rules changed the computed style"
        );
    }
    eprintln!(
        "native CSS benchmark: 4096 rules parse/index={parse_elapsed:?}, 132 node styles={:?}",
        style_started.elapsed()
    );
}

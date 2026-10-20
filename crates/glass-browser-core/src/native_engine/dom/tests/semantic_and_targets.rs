use super::*;

#[test]
fn fragment_target_uses_unique_legacy_name_after_id_precedence() {
    let document = NativeDocument::parse(
            "<a name='legacy'>Legacy</a><a name='café'>UTF-8 legacy</a><div name='not-anchor'>Not an anchor</div><div id='same'>ID wins</div><a name='same'>Name loses</a><a name='duplicate'>One</a><a name='duplicate'>Two</a><p id='duplicate-id'>ID target</p>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let legacy = document
        .node(document.fragment_target("legacy").unwrap())
        .unwrap();
    assert_eq!(legacy.element_name(), Some("a"));
    assert_eq!(legacy.attribute("name"), Some("legacy"));
    assert_eq!(
        document
            .node(document.fragment_target("café").unwrap())
            .unwrap()
            .attribute("name"),
        Some("café")
    );
    assert_eq!(
        document.fragment_target("same"),
        Some(document.resolve_target("id=same").unwrap())
    );
    assert_eq!(document.fragment_target("not-anchor"), None);
    assert_eq!(document.fragment_target("duplicate"), None);
    assert_eq!(document.fragment_target("DUPLICATE"), None);
    assert_eq!(
        document.fragment_target("duplicate-id"),
        Some(document.resolve_target("id=duplicate-id").unwrap())
    );
}

#[test]
fn text_fragment_target_stays_inside_one_visible_layout_run() {
    let document = NativeDocument::parse(
            "<p style='white-space:pre'>target phrase anchor</p><p hidden>hidden target</p><p><span>cross </span><span>run target</span></p>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let layout = document
        .layout(Viewport {
            width: 240,
            height: 100,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let first = layout
        .text_runs
        .iter()
        .find(|run| run.text == "target phrase anchor")
        .unwrap();
    let start = TextFragmentTerms {
        prefix: None,
        start: "target phrase".into(),
        end: None,
        suffix: None,
    };
    assert_eq!(
        document.text_fragment_target(&layout, &start),
        Some(first.node_id)
    );
    let range = TextFragmentTerms {
        prefix: None,
        start: "target phrase".into(),
        end: Some("anchor".into()),
        suffix: None,
    };
    assert_eq!(
        document.text_fragment_target(&layout, &range),
        Some(first.node_id)
    );
    let reversed = TextFragmentTerms {
        prefix: None,
        start: "anchor".into(),
        end: Some("target".into()),
        suffix: None,
    };
    assert_eq!(document.text_fragment_target(&layout, &reversed), None);
    let hidden = TextFragmentTerms {
        prefix: None,
        start: "hidden target".into(),
        end: None,
        suffix: None,
    };
    assert_eq!(document.text_fragment_target(&layout, &hidden), None);
    let cross_run = TextFragmentTerms {
        prefix: None,
        start: "cross".into(),
        end: Some("target".into()),
        suffix: None,
    };
    assert_eq!(document.text_fragment_target(&layout, &cross_run), None);
}

#[test]
fn dom_depth_limit_counts_open_elements_without_the_document_root() {
    let limits = NativeEngineLimits {
        // The inserted html/body elements contribute to the bounded depth.
        max_dom_depth: 3,
        ..NativeEngineLimits::default()
    };
    assert!(NativeDocument::parse("<section>one</section>", &limits).is_ok());
    assert!(matches!(
        NativeDocument::parse("<section><p>two</p></section>", &limits),
        Err(NativeEngineError::LimitExceeded { resource, .. }) if resource == "DOM depth"
    ));
}

#[test]
fn semantic_projection_infers_roles_labels_attributes_and_state() {
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse(
            r#"<label for="email">Email address</label><input id="email" type="email" placeholder="name@example.com" required><button id="save" aria-label="Save changes">Save</button><a href="/next">Next</a><input id="remember" type="checkbox" checked>"#,
            &limits,
        )
        .unwrap();

    let nodes = document.semantic_nodes();
    let email = nodes.iter().find(|node| node.tag_name == "input").unwrap();
    assert_eq!(email.role, "textbox");
    assert_eq!(email.name, "Email address");
    assert_eq!(email.input_type.as_deref(), Some("email"));
    assert_eq!(email.empty, Some(true));
    assert!(email.required);
    assert_eq!(
        email.reference,
        format!("ref=r1:n{}", email.node_id.index())
    );
    assert_eq!(
        document.node(email.node_id).unwrap().attribute("ID"),
        Some("email")
    );

    let save = nodes.iter().find(|node| node.role == "button").unwrap();
    assert_eq!(save.name, "Save changes");
    assert_eq!(document.resolve_target("id=save"), Ok(save.node_id));
    assert_eq!(
        document.resolve_target("role=button[name=Save changes]"),
        Ok(save.node_id)
    );

    let checkbox = nodes.iter().find(|node| node.role == "checkbox").unwrap();
    assert_eq!(checkbox.checked, Some(true));
    assert!(
        nodes
            .iter()
            .any(|node| node.role == "link" && node.name == "Next")
    );
}

#[test]
fn locators_reject_duplicates_stale_references_and_unknown_forms() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(
        "<button>Save</button><button>Save</button><input id='name'>",
        &limits,
    )
    .unwrap();
    assert!(matches!(
        document.resolve_target("role=button[name=Save]"),
        Err(NativeEngineError::AmbiguousTarget { matches: 2 })
    ));
    let name_node = document
        .semantic_nodes()
        .into_iter()
        .find(|node| node.role == "textbox")
        .unwrap();
    let reference = name_node.reference;
    assert!(document.resolve_target(&reference).is_ok());
    document.revision = 2;
    assert!(matches!(
        document.resolve_target(&reference),
        Err(NativeEngineError::DetachedTarget)
    ));
    assert_eq!(document.resolve_target("css=#name"), Ok(name_node.node_id));
}

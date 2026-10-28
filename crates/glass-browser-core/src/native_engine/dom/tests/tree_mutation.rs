use super::*;

#[test]
fn native_document_subtree_transfer_is_identity_mapped_and_atomic() {
    let limits = NativeEngineLimits::default();
    let mut source = NativeDocument::parse(
            "<main><section id='moving' data-kept='yes'><span id='child'>kept</span><template id='nested'><b id='inside'>inert</b></template></section></main>",
            &limits,
        )
        .unwrap();
    let mut destination = NativeDocument::parse("<main id='target'></main>", &limits).unwrap();
    let source_root = source.find_element_by_id("moving").unwrap();
    let source_parent = source.node(source_root).unwrap().parent().unwrap();
    let identities = transfer_identities(&source, source_root);
    let root_temporary_index = identities[0].destination_temporary_index;
    let old_source = source.clone();
    let old_destination = destination.clone();
    let incomplete = &identities[..identities.len() - 1];
    let mut rejected_source = source.clone();
    let mut rejected_destination = destination.clone();
    assert!(
        rejected_source
            .transfer_script_node_to(&mut rejected_destination, source_root.index, incomplete,)
            .is_err()
    );
    assert_eq!(source, old_source);
    assert_eq!(destination, old_destination);

    source
        .transfer_script_node_to(&mut destination, source_root.index, &identities)
        .unwrap();
    assert_eq!(source.revision(), old_source.revision() + 1);
    assert_eq!(destination.revision(), old_destination.revision() + 1);
    assert!(source.node(source_root).is_none());
    assert!(
        !source
            .raw_node(source_parent)
            .unwrap()
            .children()
            .contains(&source_root)
    );

    let target = destination.find_element_by_id("target").unwrap();
    let moved_root = destination.script_node_ids[&root_temporary_index];
    let moved_element = destination.raw_node(moved_root).unwrap();
    assert_eq!(moved_element.attribute("data-kept"), Some("yes"));
    assert_eq!(moved_element.parent(), None);
    let template = identities
        .iter()
        .map(|identity| destination.script_node_ids[&identity.destination_temporary_index])
        .find(|id| destination.raw_node(*id).unwrap().element_name() == Some("template"))
        .unwrap();
    let content = destination
        .raw_node(template)
        .unwrap()
        .template_content
        .unwrap();
    let content_child = destination.raw_node(content).unwrap().children()[0];
    assert_eq!(
        destination
            .raw_node(content_child)
            .and_then(NativeNode::element_name),
        Some("b")
    );

    destination
        .apply_script_commands(&[NativeScriptCommand::AppendChild {
            parent_index: target.index,
            child_index: root_temporary_index,
        }])
        .unwrap();
    assert_eq!(destination.node(moved_root).unwrap().parent(), Some(target));
    // Template contents belong to a separate inert document fragment and are
    // intentionally excluded from document.getElementById-style lookup.
    assert!(destination.find_element_by_id("inside").is_none());
    assert!(
        destination
            .raw_node(content_child)
            .is_some_and(|node| node.attribute("id") == Some("inside"))
    );
}

#[test]
fn native_document_subtree_transfer_preserves_shadow_root_host_links() {
    let limits = NativeEngineLimits::default();
    let mut source = NativeDocument::parse(
        "<main><x-host id='moving'>LIGHT_TEXT</x-host></main>",
        &limits,
    )
    .unwrap();
    let mut destination = NativeDocument::parse("<main id='target'></main>", &limits).unwrap();
    let host = source.find_element_by_id("moving").unwrap();
    let shadow_root_index = u32::MAX - 3;
    let child_index = u32::MAX - 2;
    let text_index = u32::MAX - 1;
    source
        .apply_script_commands(&[
            NativeScriptCommand::CreateShadowRoot {
                host_index: host.index(),
                node_index: shadow_root_index,
                mode: "closed".into(),
            },
            NativeScriptCommand::CreateElement {
                node_index: child_index,
                tag_name: "span".into(),
                namespace_uri: None,
            },
            NativeScriptCommand::CreateTextNode {
                node_index: text_index,
                value: "SHADOW_TEXT".into(),
            },
            NativeScriptCommand::AppendChild {
                parent_index: child_index,
                child_index: text_index,
            },
            NativeScriptCommand::AppendChild {
                parent_index: shadow_root_index,
                child_index,
            },
        ])
        .unwrap();
    let shadow_root = source.raw_node(host).unwrap().shadow_root.unwrap();
    let identities = transfer_identities(&source, host);
    let temporary_index_for = |id: NativeNodeId| {
        identities
            .iter()
            .find(|identity| identity.source_index == id.index())
            .expect("every shadow-including descendant has an identity")
            .destination_temporary_index
    };
    let host_temporary_index = temporary_index_for(host);
    let shadow_root_temporary_index = temporary_index_for(shadow_root);

    source
        .transfer_script_node_to(&mut destination, host.index(), &identities)
        .expect("host and its shadow-including subtree transfer atomically");
    assert!(source.raw_node(host).unwrap().shadow_root.is_none());
    assert!(source.raw_node(shadow_root).unwrap().shadow_host.is_none());
    let moved_host = destination.script_node_ids[&host_temporary_index];
    let moved_shadow_root = destination.script_node_ids[&shadow_root_temporary_index];
    assert_eq!(
        destination.raw_node(moved_host).unwrap().shadow_root,
        Some(moved_shadow_root)
    );
    let moved_root_node = destination.raw_node(moved_shadow_root).unwrap();
    assert_eq!(moved_root_node.shadow_host, Some(moved_host));
    assert_eq!(moved_root_node.shadow_mode.as_deref(), Some("closed"));

    let destination_host = destination.find_element_by_id("target").unwrap();
    destination
        .apply_script_commands(&[NativeScriptCommand::AppendChild {
            parent_index: destination_host.index(),
            child_index: host_temporary_index,
        }])
        .unwrap();
    let (visible, truncated) = destination.visible_text(1024);
    assert!(!truncated);
    assert!(visible.contains("SHADOW_TEXT"));
    assert!(!visible.contains("LIGHT_TEXT"));

    let root_identities = transfer_identities(&destination, moved_shadow_root);
    let old_destination = destination.clone();
    assert!(
        destination
            .transfer_script_node_to(&mut source, moved_shadow_root.index(), &root_identities)
            .is_err(),
        "a ShadowRoot itself is not adoptable"
    );
    assert_eq!(destination, old_destination);
}

#[test]
fn native_document_subtree_transfer_rejects_destination_capacity_atomically() {
    let source_limits = NativeEngineLimits::default();
    let destination_limits = NativeEngineLimits {
        max_nodes: 5,
        ..NativeEngineLimits::default()
    };
    let mut source = NativeDocument::parse(
        "<main><section id='moving'><span>child</span></section></main>",
        &source_limits,
    )
    .unwrap();
    let mut destination =
        NativeDocument::parse("<main id='target'></main>", &destination_limits).unwrap();
    let root = source.find_element_by_id("moving").unwrap();
    let identities = transfer_identities(&source, root);
    let old_source = source.clone();
    let old_destination = destination.clone();

    assert!(
        source
            .transfer_script_node_to(&mut destination, root.index, &identities)
            .is_err()
    );
    assert_eq!(source, old_source);
    assert_eq!(destination, old_destination);
}

#[test]
fn native_document_multi_root_transfer_is_atomic_and_identity_mapped() {
    let limits = NativeEngineLimits::default();
    let mut source = NativeDocument::parse(
        "<main><p id='first'>one<span>nested</span></p><p id='second'>two</p></main>",
        &limits,
    )
    .unwrap();
    let mut destination = NativeDocument::parse("<main id='target'></main>", &limits).unwrap();
    let first = source.find_element_by_id("first").unwrap();
    let second = source.find_element_by_id("second").unwrap();
    let first_parent = source.raw_node(first).unwrap().parent().unwrap();
    let first_child = source.raw_node(first).unwrap().children()[0];
    let first_identities = transfer_identities(&source, first);
    let mut second_identities = transfer_identities(&source, second);
    for identity in &mut second_identities {
        identity.destination_temporary_index =
            identity.destination_temporary_index.saturating_sub(64);
    }
    let transfers = vec![
        NativeNodeSubtreeTransfer {
            source_index: first.index,
            identities: first_identities,
        },
        NativeNodeSubtreeTransfer {
            source_index: second.index,
            identities: second_identities,
        },
    ];
    let source_before = source.clone();
    let destination_before = destination.clone();
    let mut overlapping = transfers.clone();
    let nested = source.find_element_by_id("first").unwrap();
    let nested_child = source
        .raw_node(nested)
        .unwrap()
        .children()
        .iter()
        .copied()
        .find(|id| source.raw_node(*id).unwrap().element_name().is_some())
        .unwrap();
    let mut overlapping_identities = transfer_identities(&source, nested_child);
    for identity in &mut overlapping_identities {
        identity.destination_temporary_index =
            identity.destination_temporary_index.saturating_sub(128);
    }
    overlapping.push(NativeNodeSubtreeTransfer {
        source_index: nested_child.index,
        identities: overlapping_identities,
    });
    let mut failed_source = source.clone();
    let mut failed_destination = destination.clone();
    assert!(
        failed_source
            .transfer_script_nodes_to(&mut failed_destination, &overlapping)
            .is_err()
    );
    assert_eq!(failed_source, source_before);
    assert_eq!(failed_destination, destination_before);

    source
        .transfer_script_nodes_to(&mut destination, &transfers)
        .unwrap();
    assert_eq!(source.revision(), source_before.revision() + 1);
    assert_eq!(destination.revision(), destination_before.revision() + 1);
    assert!(source.node(first).is_none());
    assert!(source.node(second).is_none());
    assert!(
        !source
            .raw_node(first_parent)
            .unwrap()
            .children()
            .contains(&first)
    );
    let destination_first =
        destination.script_node_ids[&transfers[0].identities[0].destination_temporary_index];
    let snapshot = destination.script_snapshot_for_viewport(1024, Viewport::default());
    assert!(snapshot.nodes.iter().any(
            |node| node.node_index == destination_first.index() && node.parent_index.is_none()
        ));
    assert!(
        snapshot
            .elements
            .iter()
            .any(|element| element.node_index == destination_first.index())
    );
    let first_child_identity = transfers[0]
        .identities
        .iter()
        .find(|identity| identity.source_index == first_child.index)
        .unwrap();
    let destination_first_child =
        destination.script_node_ids[&first_child_identity.destination_temporary_index];
    assert_eq!(
        destination
            .raw_node(destination_first_child)
            .unwrap()
            .parent(),
        Some(destination_first)
    );
    assert_eq!(
        destination
            .raw_node(destination_first)
            .unwrap()
            .element_name(),
        Some("p")
    );
    assert_eq!(
        destination
            .raw_node(
                destination.script_node_ids
                    [&transfers[1].identities[0].destination_temporary_index]
            )
            .unwrap()
            .element_name(),
        Some("p")
    );
    NativeDocument::from_content_wire(source.to_content_wire(), &limits, source.generation())
        .unwrap();
    NativeDocument::from_content_wire(
        destination.to_content_wire(),
        &limits,
        destination.generation(),
    )
    .unwrap();
}

#[test]
fn script_document_title_materializes_missing_title_node() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse("<body>Content</body>", &limits).unwrap();

    document
        .apply_script_commands(&[NativeScriptCommand::SetDocumentTitle {
            value: "Native title".into(),
        }])
        .unwrap();

    let title = document.find_element(document.root, "title").unwrap();
    assert_eq!(document.title(1024), ("Native title".into(), false));
    let head = document.find_element(document.root, "head").unwrap();
    assert_eq!(document.node(title).unwrap().parent(), Some(head));
    assert_eq!(
        document.element_text(title, 1024),
        Some(("Native title".into(), false))
    );
}

#[test]
fn script_text_content_replaces_subtree_and_detaches_old_nodes() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(
        "<body><p id='root'>before <span id='old'>child</span></p></body>",
        &limits,
    )
    .unwrap();
    let root = document.find_element_by_id("root").unwrap();
    let old = document.find_element_by_id("old").unwrap();

    document
        .apply_script_commands(&[NativeScriptCommand::SetTextContent {
            node_index: root.index(),
            value: "after & literal".into(),
        }])
        .unwrap();

    assert_eq!(
        document.element_text(root, 1024),
        Some(("after & literal".into(), false))
    );
    assert!(document.find_element_by_id("old").is_none());
    assert!(document.node(old).is_none());
    assert_eq!(
        document.visible_text(1024),
        ("after & literal".into(), false)
    );
    assert_eq!(
        document
            .script_snapshot_for_viewport(1024, Viewport::default())
            .elements
            .len(),
        4
    );
}

#[test]
fn script_inner_html_replaces_subtree_and_serializes_markup() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(
        "<body><p id='root'>before <span id='old'>child</span></p></body>",
        &limits,
    )
    .unwrap();
    let root = document.find_element_by_id("root").unwrap();
    let old = document.find_element_by_id("old").unwrap();

    document
        .apply_script_commands(&[NativeScriptCommand::SetInnerHtml {
            node_index: root.index(),
            value: "<strong id='new'>after &amp; literal</strong> tail".into(),
        }])
        .unwrap();

    let new = document.find_element_by_id("new").unwrap();
    assert_eq!(
        document.element_text(root, 1024),
        Some(("after & literal tail".into(), false))
    );
    assert_eq!(
        document.element_inner_html(root, 1024),
        "<strong id=\"new\">after &amp; literal</strong> tail"
    );
    assert_eq!(document.node(old), None);
    assert_eq!(document.node(new).and_then(NativeNode::parent), Some(root));
}

#[test]
fn html_template_contents_use_an_inert_fragment_outside_document_traversal() {
    let document = NativeDocument::parse(
            "<body><template id='template'><span id='inside'>private</span></template><p>visible</p></body>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let template = document.find_element_by_id("template").unwrap();
    let fragment = document
        .node(template)
        .and_then(|node| node.template_content)
        .expect("template has its stable content fragment");
    let fragment_node = document.node(fragment).expect("connected template content");
    let inside = *fragment_node.children().first().expect("template child");

    assert!(matches!(
        fragment_node.kind(),
        NativeNodeKind::DocumentFragment
    ));
    assert_eq!(fragment_node.parent(), None);
    assert_eq!(
        document.node(inside).and_then(NativeNode::parent),
        Some(fragment)
    );
    assert!(!document.is_attached(inside));
    assert!(document.node(inside).is_some());
    assert!(document.find_element_by_id("inside").is_none());
    assert_eq!(
        document.element_inner_html(template, 1024),
        "<span id=\"inside\">private</span>"
    );
    assert!(!document.element_node_ids().any(|node| node == inside));

    let snapshot = document.script_snapshot_for_viewport(1024, Viewport::default());
    assert!(snapshot.nodes.iter().any(|node| {
        node.node_index == fragment.index()
            && node.node_type == 11
            && node.parent_index.is_none()
            && node.template_host_index == Some(template.index())
    }));
    assert!(snapshot.elements.iter().any(|element| {
        element.node_index == template.index()
            && element.template_content_index == Some(fragment.index())
    }));
}

#[test]
fn script_can_mutate_template_content_without_replacing_its_fragment() {
    let mut document = NativeDocument::parse(
        "<body><template id='template'><i id='old'>old</i></template></body>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let template = document.find_element_by_id("template").unwrap();
    let fragment = document.node(template).unwrap().template_content.unwrap();

    document
        .apply_script_commands(&[NativeScriptCommand::SetInnerHtml {
            node_index: template.index(),
            value: "<b id='new'>new</b>".into(),
        }])
        .unwrap();

    let template_node = document.node(template).unwrap();
    let fragment_node = document.node(fragment).unwrap();
    let child = *fragment_node.children().first().expect("replacement child");
    assert_eq!(template_node.template_content, Some(fragment));
    assert_eq!(
        document.node(child).and_then(NativeNode::parent),
        Some(fragment)
    );
    assert_eq!(
        document.node(child).and_then(NativeNode::element_name),
        Some("b")
    );
    assert_eq!(
        document.element_inner_html(template, 1024),
        "<b id=\"new\">new</b>"
    );
    assert!(document.find_element_by_id("new").is_none());
}

#[test]
fn script_inner_html_node_limit_failure_preserves_the_existing_subtree() {
    let mut document = NativeDocument::parse(
        "<main><p id='root'>old <strong>content</strong></p></main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let root = document.find_element_by_id("root").unwrap();
    let original = document.element_inner_html(root, 1024);
    document.max_nodes = document.nodes.len();

    let result = document.apply_script_commands(&[NativeScriptCommand::SetInnerHtml {
        node_index: root.index(),
        value: "<span>replacement</span>".into(),
    }]);

    assert!(result.is_err(), "the configured node budget is exhausted");
    assert_eq!(document.element_inner_html(root, 1024), original);
}

#[test]
fn script_inner_html_depth_limit_failure_preserves_the_existing_subtree() {
    let mut document = NativeDocument::parse(
        "<main><p id='root'>old <strong>content</strong></p></main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let root = document.find_element_by_id("root").unwrap();
    let original = document.element_inner_html(root, 1024);
    document.max_dom_depth = document.element_depth(root).saturating_add(1);

    let result = document.apply_script_commands(&[NativeScriptCommand::SetInnerHtml {
        node_index: root.index(),
        value: "<span><b>replacement</b></span>".into(),
    }]);

    assert!(result.is_err(), "the replacement exceeds the DOM depth cap");
    assert_eq!(document.element_inner_html(root, 1024), original);
}

#[test]
fn script_inner_html_fosters_table_text_and_elements_before_nested_table() {
    let mut document = NativeDocument::parse(
        "<main id='root'><p id='old'>old</p></main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let root = document.find_element_by_id("root").unwrap();

    document
            .apply_script_commands(&[NativeScriptCommand::SetInnerHtml {
                node_index: root.index(),
                value: "<table><tbody id='section'>before<div id='foster'><span id='nested'>inside</span></div> &#32;<!--kept--><tr><td>cell</td></tr></tbody></table>".into(),
            }])
            .unwrap();

    let foster = document.find_element_by_id("foster").unwrap();
    let nested = document.find_element_by_id("nested").unwrap();
    let section = document.find_element_by_id("section").unwrap();
    let table = document.node(section).and_then(NativeNode::parent).unwrap();
    assert_eq!(
        document.node(foster).and_then(NativeNode::parent),
        Some(root)
    );
    assert_eq!(
        document.node(nested).and_then(NativeNode::parent),
        Some(foster)
    );
    assert_eq!(
        document.node(table).and_then(NativeNode::parent),
        Some(root)
    );

    let root_children = document.node(root).unwrap().children();
    assert_eq!(root_children.len(), 3);
    assert!(matches!(
        document.node(root_children[0]).map(NativeNode::kind),
        Some(NativeNodeKind::Text(value)) if value == "before"
    ));
    assert_eq!(
        document
            .node(root_children[1])
            .and_then(NativeNode::element_name),
        Some("div")
    );
    assert_eq!(
        document
            .node(root_children[2])
            .and_then(NativeNode::element_name),
        Some("table")
    );

    let section_children = document.node(section).unwrap().children();
    assert!(matches!(
        document.node(section_children[0]).map(NativeNode::kind),
        Some(NativeNodeKind::Text(value)) if value == "  "
    ));
    assert!(matches!(
        document.node(section_children[1]).map(NativeNode::kind),
        Some(NativeNodeKind::Comment(value)) if value == "kept"
    ));
    assert_eq!(
        document.element_inner_html(section, 1024),
        "  <!--kept--><tr><td>cell</td></tr>"
    );
}

#[test]
fn script_inner_html_foster_parenting_stays_inside_table_fragment_context() {
    let mut document = NativeDocument::parse(
        "<table id='target'><caption>old</caption></table>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let target = document.find_element_by_id("target").unwrap();

    document
        .apply_script_commands(&[NativeScriptCommand::SetInnerHtml {
            node_index: target.index(),
            value:
                "<tbody id='section'>start<div id='foster'>inside</div> &#32;<!--kept--></tbody>"
                    .into(),
        }])
        .unwrap();

    let foster = document.find_element_by_id("foster").unwrap();
    let section = document.find_element_by_id("section").unwrap();
    assert_eq!(
        document.node(foster).and_then(NativeNode::parent),
        Some(target)
    );
    assert_eq!(
        document.node(section).and_then(NativeNode::parent),
        Some(target)
    );
    let body = document.node(target).and_then(NativeNode::parent).unwrap();
    assert_eq!(document.node(body).unwrap().children().len(), 1);
    assert!(matches!(
        document.node(document.node(section).unwrap().children()[0]).map(NativeNode::kind),
        Some(NativeNodeKind::Text(value)) if value == "  "
    ));
    assert_eq!(
        document.element_inner_html(target, 1024),
        "<tbody id=\"section\">  <!--kept--></tbody>start<div id=\"foster\">inside</div>"
    );
}

#[test]
fn script_inner_html_reprocesses_svg_table_as_html_and_fosters_children() {
    let mut document =
        NativeDocument::parse("<svg id='svg'></svg>", &NativeEngineLimits::default()).unwrap();
    let svg = document.find_element_by_id("svg").unwrap();

    document
        .apply_script_commands(&[NativeScriptCommand::SetInnerHtml {
            node_index: svg.index(),
            value: "<table id='svg-table'><div id='svg-child'>inside</div></table>".into(),
        }])
        .unwrap();

    let table = document.find_element_by_id("svg-table").unwrap();
    let child = document.find_element_by_id("svg-child").unwrap();
    assert_eq!(document.node(table).and_then(NativeNode::parent), Some(svg));
    assert_eq!(
        document.node(table).and_then(NativeNode::namespace_uri),
        Some(HTML_NAMESPACE_URI)
    );
    assert_eq!(
        document.node(child).and_then(NativeNode::namespace_uri),
        Some(HTML_NAMESPACE_URI)
    );
    assert_eq!(document.node(child).and_then(NativeNode::parent), Some(svg));
    assert!(document.node(table).unwrap().children().is_empty());
    assert_eq!(document.node(svg).unwrap().children(), &[child, table]);
}

#[test]
fn script_remove_detaches_existing_subtree() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(
            "<body><section id='remove'><span id='nested'>gone</span></section><p id='keep'>stay</p></body>",
            &limits,
        )
        .unwrap();
    let remove = document.find_element_by_id("remove").unwrap();
    let nested = document.find_element_by_id("nested").unwrap();

    document
        .apply_script_commands(&[NativeScriptCommand::RemoveNode {
            node_index: remove.index(),
        }])
        .unwrap();

    assert!(document.find_element_by_id("remove").is_none());
    assert!(document.find_element_by_id("nested").is_none());
    assert!(document.node(remove).is_none());
    assert!(document.node(nested).is_none());
    assert_eq!(
        document.raw_node(remove).unwrap().children(),
        &[nested],
        "a detached node must retain its child subtree"
    );
    assert_eq!(
        document.raw_node(nested).and_then(NativeNode::parent),
        Some(remove)
    );
    assert_eq!(document.visible_text(1024), ("stay".into(), false));
}

#[test]
fn script_create_element_and_insert_child_commits_owned_nodes() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(
        "<body><p id='before'>before</p><p id='after'>after</p></body>",
        &limits,
    )
    .unwrap();
    let body = document.find_element(document.root(), "body").unwrap();
    let first = u32::MAX - 1;
    let child = u32::MAX - 2;
    let inserted = u32::MAX - 3;
    let child_text = u32::MAX - 4;

    document
        .apply_script_commands(&[
            NativeScriptCommand::CreateElement {
                node_index: first,
                tag_name: "section".into(),
                namespace_uri: None,
            },
            NativeScriptCommand::CreateElement {
                node_index: child,
                tag_name: "strong".into(),
                namespace_uri: None,
            },
            NativeScriptCommand::CreateTextNode {
                node_index: child_text,
                value: "created text".into(),
            },
            NativeScriptCommand::SetAttribute {
                node_index: first,
                name: "id".into(),
                value: "created".into(),
                namespace_uri: None,
            },
            NativeScriptCommand::AppendChild {
                parent_index: child,
                child_index: child_text,
            },
            NativeScriptCommand::AppendChild {
                parent_index: first,
                child_index: child,
            },
            NativeScriptCommand::AppendChild {
                parent_index: body.index(),
                child_index: first,
            },
            NativeScriptCommand::CreateElement {
                node_index: inserted,
                tag_name: "em".into(),
                namespace_uri: None,
            },
            NativeScriptCommand::SetTextContent {
                node_index: inserted,
                value: "inserted".into(),
            },
            NativeScriptCommand::InsertBefore {
                parent_index: body.index(),
                child_index: inserted,
                before_index: Some(first),
            },
        ])
        .unwrap();

    let created = document.find_element_by_id("created").unwrap();
    let created_child = document.find_element(created, "strong").unwrap();
    let inserted_node = document.find_element(body, "em").unwrap();
    assert_eq!(
        document.element_text(created, 1024),
        Some(("created text".into(), false))
    );
    assert_eq!(
        document.element_inner_html(created, 1024),
        "<strong>created text</strong>"
    );
    assert_eq!(
        document.node(created_child).and_then(NativeNode::parent),
        Some(created)
    );
    assert_eq!(
        document.node(inserted_node).and_then(NativeNode::parent),
        Some(body)
    );
    assert_eq!(
        document
            .node(body)
            .unwrap()
            .children()
            .iter()
            .position(|id| *id == inserted_node),
        document
            .node(body)
            .unwrap()
            .children()
            .iter()
            .position(|id| *id == created)
            .map(|position| position.saturating_sub(1))
    );
}

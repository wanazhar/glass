use super::*;
#[cfg(test)]
use std::borrow::Cow;

#[cfg(test)]
pub(super) fn should_auto_close(current: &str, next: &str) -> bool {
    (current == "li" && next == "li")
        || (current == "p"
            && matches!(
                next,
                "address"
                    | "article"
                    | "aside"
                    | "blockquote"
                    | "details"
                    | "div"
                    | "dl"
                    | "fieldset"
                    | "figcaption"
                    | "figure"
                    | "footer"
                    | "form"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "header"
                    | "hgroup"
                    | "hr"
                    | "main"
                    | "menu"
                    | "nav"
                    | "ol"
                    | "p"
                    | "pre"
                    | "section"
                    | "table"
                    | "ul"
            ))
        || (matches!(current, "dt" | "dd") && matches!(next, "dt" | "dd"))
        || (current == "rt" && next == "rt")
        || (current == "rp" && next == "rp")
        || (current == "option" && matches!(next, "option" | "optgroup"))
        || (current == "optgroup" && next == "optgroup")
        || (current == "tr" && matches!(next, "tr" | "tbody" | "thead" | "tfoot"))
        || (matches!(current, "td" | "th")
            && matches!(next, "td" | "th" | "tr" | "tbody" | "thead" | "tfoot"))
        || (current == "thead" && matches!(next, "tbody" | "tfoot"))
        || (current == "tbody" && matches!(next, "tbody" | "tfoot"))
        || (current == "tfoot" && next == "tbody")
        || (current == "colgroup" && matches!(next, "colgroup" | "tbody" | "thead" | "tfoot"))
}

#[cfg(test)]
pub(super) fn prepare_html_table_cell_structural_start_token(
    document: &NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    fragment_root: Option<NativeNodeId>,
    incoming_name: &str,
) -> Result<(), NativeEngineError> {
    if !matches!(
        incoming_name,
        "caption" | "col" | "colgroup" | "tbody" | "td" | "tfoot" | "th" | "thead" | "tr"
    ) {
        return Ok(());
    }
    let current_id = stack
        .last()
        .copied()
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node while processing an in-cell start".into(),
        })?;
    let current = document
        .node(current_id)
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node while processing an in-cell start".into(),
        })?;
    if current.namespace_uri() != Some(HTML_NAMESPACE_URI) {
        return Ok(());
    }

    let mut cell_index = None;
    let mut table_index = None;
    for index in (0..stack.len()).rev() {
        let id = stack[index];
        if Some(id) == fragment_root {
            break;
        }
        let node = document.node(id).ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder in-cell scope references an unknown node".into(),
        })?;
        if node.namespace_uri() != Some(HTML_NAMESPACE_URI) {
            if node.namespace_uri() == Some(SVG_NAMESPACE_URI)
                && node.element_name() == Some("foreignobject")
            {
                break;
            }
            continue;
        }
        match node.element_name() {
            Some("table") => {
                if cell_index.is_some() {
                    table_index = Some(index);
                }
                break;
            }
            Some("html" | "template") => break,
            Some("td" | "th") if cell_index.is_none() => cell_index = Some(index),
            _ => {}
        }
    }

    let (Some(cell_index), Some(table_index)) = (cell_index, table_index) else {
        return Ok(());
    };
    stack.truncate(cell_index);
    if matches!(
        incoming_name,
        "caption" | "col" | "colgroup" | "tbody" | "tfoot" | "thead"
    ) {
        stack.truncate(table_index + 1);
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn is_table_special_start_tag(
    name: &str,
    attributes: &BTreeMap<String, String>,
) -> bool {
    matches!(
        name,
        "caption"
            | "col"
            | "colgroup"
            | "tbody"
            | "tfoot"
            | "thead"
            | "tr"
            | "td"
            | "th"
            | "table"
            | "style"
            | "script"
            | "template"
            | "form"
    ) || (name == "input"
        && attributes
            .get("type")
            .is_some_and(|value| value.eq_ignore_ascii_case("hidden")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
pub(super) enum ActiveFormattingEntry {
    Marker,
    Element(NativeNodeId),
}

#[cfg(test)]
pub(super) type ActiveFormattingList = Vec<ActiveFormattingEntry>;

#[cfg(test)]
pub(super) fn is_html_formatting_element(name: &str) -> bool {
    matches!(
        name,
        "a" | "b"
            | "big"
            | "code"
            | "em"
            | "font"
            | "i"
            | "nobr"
            | "s"
            | "small"
            | "strike"
            | "strong"
            | "tt"
            | "u"
    )
}

#[cfg(test)]
pub(super) fn is_active_formatting_marker_element(name: &str) -> bool {
    matches!(
        name,
        "applet" | "caption" | "marquee" | "object" | "template" | "td" | "th"
    )
}

#[cfg(test)]
pub(super) fn is_marker_sensitive_end_tag(name: &str) -> bool {
    matches!(
        name,
        "applet"
            | "caption"
            | "marquee"
            | "object"
            | "table"
            | "tbody"
            | "td"
            | "template"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
    )
}

#[cfg(test)]
pub(super) fn clear_active_formatting_to_marker(active: &mut ActiveFormattingList) {
    while let Some(entry) = active.pop() {
        if entry == ActiveFormattingEntry::Marker {
            break;
        }
    }
}

#[cfg(test)]
pub(super) fn clear_markers_for_popped_elements(
    document: &NativeDocument,
    stack_before: &[NativeNodeId],
    stack_after: &[NativeNodeId],
    active: &mut ActiveFormattingList,
) {
    for id in stack_before
        .get(stack_after.len()..)
        .unwrap_or_default()
        .iter()
        .rev()
    {
        if document
            .raw_node(*id)
            .and_then(NativeNode::element_name)
            .is_some_and(is_active_formatting_marker_element)
        {
            clear_active_formatting_to_marker(active);
        }
    }
}

#[cfg(test)]
pub(super) fn active_formatting_entry_index(
    document: &NativeDocument,
    active: &ActiveFormattingList,
    name: &str,
) -> Option<usize> {
    let start = active
        .iter()
        .rposition(|entry| *entry == ActiveFormattingEntry::Marker)
        .map_or(0, |index| index + 1);
    active[start..]
        .iter()
        .rposition(|entry| match entry {
            ActiveFormattingEntry::Marker => false,
            ActiveFormattingEntry::Element(id) => document.raw_node(*id).is_some_and(|node| {
                node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                    && node.element_name() == Some(name)
            }),
        })
        .map(|index| start + index)
}

#[cfg(test)]
pub(super) fn push_active_formatting_element(
    document: &NativeDocument,
    active: &mut ActiveFormattingList,
    id: NativeNodeId,
) -> Result<(), NativeEngineError> {
    let node = document
        .raw_node(id)
        .ok_or(NativeEngineError::DetachedTarget)?;
    if node.namespace_uri() != Some(HTML_NAMESPACE_URI) {
        return Ok(());
    }
    let Some(name) = node.element_name() else {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: "active formatting entry is not an element".into(),
        });
    };
    let attributes = node.attributes().ok_or_else(|| NativeEngineError::Parse {
        offset: 0,
        reason: "active formatting element has no attributes".into(),
    })?;
    let marker_start = active
        .iter()
        .rposition(|entry| *entry == ActiveFormattingEntry::Marker)
        .map_or(0, |index| index + 1);
    let equivalent_entries = active[marker_start..]
        .iter()
        .enumerate()
        .filter_map(|(relative_index, entry)| {
            let ActiveFormattingEntry::Element(existing_id) = entry else {
                return None;
            };
            document.raw_node(*existing_id).and_then(|existing| {
                (existing.namespace_uri() == Some(HTML_NAMESPACE_URI)
                    && existing.element_name() == Some(name)
                    && existing.attributes() == Some(attributes))
                .then_some(marker_start + relative_index)
            })
        })
        .collect::<Vec<_>>();
    if equivalent_entries.len() >= 3 {
        active.remove(equivalent_entries[0]);
    }
    active.push(ActiveFormattingEntry::Element(id));
    Ok(())
}

#[cfg(test)]
pub(super) fn is_html_special_element(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "applet"
            | "area"
            | "article"
            | "aside"
            | "base"
            | "basefont"
            | "bgsound"
            | "blockquote"
            | "body"
            | "br"
            | "button"
            | "caption"
            | "center"
            | "col"
            | "colgroup"
            | "dd"
            | "details"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "embed"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "frame"
            | "frameset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hgroup"
            | "hr"
            | "html"
            | "iframe"
            | "img"
            | "input"
            | "keygen"
            | "li"
            | "link"
            | "listing"
            | "main"
            | "marquee"
            | "menu"
            | "meta"
            | "nav"
            | "noembed"
            | "noframes"
            | "noscript"
            | "object"
            | "ol"
            | "p"
            | "param"
            | "plaintext"
            | "pre"
            | "script"
            | "search"
            | "section"
            | "select"
            | "source"
            | "style"
            | "summary"
            | "table"
            | "tbody"
            | "td"
            | "template"
            | "textarea"
            | "tfoot"
            | "th"
            | "thead"
            | "title"
            | "tr"
            | "track"
            | "ul"
            | "wbr"
            | "xmp"
    )
}

#[cfg(test)]
pub(super) fn is_special_tree_builder_element(document: &NativeDocument, id: NativeNodeId) -> bool {
    document
        .raw_node(id)
        .is_some_and(|node| match node.namespace_uri() {
            Some(HTML_NAMESPACE_URI) => node.element_name().is_some_and(is_html_special_element),
            Some(MATHML_NAMESPACE_URI) => node.element_name().is_some_and(|name| {
                matches!(name, "mi" | "mo" | "mn" | "ms" | "mtext" | "annotation-xml")
            }),
            Some(SVG_NAMESPACE_URI) => node
                .element_name()
                .is_some_and(|name| matches!(name, "foreignobject" | "desc" | "title")),
            _ => false,
        })
}

#[cfg(test)]
pub(super) fn parser_uses_foreign_character_rules(
    document: &NativeDocument,
    id: NativeNodeId,
) -> bool {
    let Some(node) = document.raw_node(id) else {
        return false;
    };
    match node.namespace_uri() {
        None | Some(HTML_NAMESPACE_URI) => false,
        Some(SVG_NAMESPACE_URI) => !node_is_html_integration_point(node),
        Some(MATHML_NAMESPACE_URI) => {
            !node_is_mathml_text_integration_point(node) && !node_is_html_integration_point(node)
        }
        Some(_) => true,
    }
}

#[cfg(test)]
pub(super) fn foreign_content_breakout_start_tag(
    name: &str,
    attributes: &BTreeMap<String, String>,
) -> bool {
    matches!(
        name,
        "b" | "big"
            | "blockquote"
            | "body"
            | "br"
            | "center"
            | "code"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "em"
            | "embed"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "hr"
            | "i"
            | "img"
            | "li"
            | "listing"
            | "menu"
            | "meta"
            | "nobr"
            | "ol"
            | "p"
            | "pre"
            | "ruby"
            | "s"
            | "small"
            | "span"
            | "strong"
            | "strike"
            | "sub"
            | "sup"
            | "table"
            | "tt"
            | "u"
            | "ul"
            | "var"
    ) || (name == "font"
        && ["color", "face", "size"]
            .iter()
            .any(|key| attributes.contains_key(*key)))
}

#[cfg(test)]
pub(super) fn prepare_foreign_content_breakout_start_tag(
    document: &NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    name: &str,
    attributes: &BTreeMap<String, String>,
    active_formatting: &mut ActiveFormattingList,
) -> Result<bool, NativeEngineError> {
    if !foreign_content_breakout_start_tag(name, attributes) {
        return Ok(false);
    }
    let Some(current) = stack.last().copied() else {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: "foreign-content breakout lost the open-elements stack".into(),
        });
    };
    if !parser_uses_foreign_character_rules(document, current) {
        return Ok(false);
    }

    let stack_before = stack.clone();
    while stack.len() > 1 {
        let current = *stack.last().expect("non-empty open-elements stack");
        if !parser_uses_foreign_character_rules(document, current) {
            break;
        }
        stack.pop();
    }
    clear_markers_for_popped_elements(document, &stack_before, stack, active_formatting);
    Ok(true)
}

#[cfg(test)]
pub(super) fn parser_uses_foreign_end_tag_rules(
    document: &NativeDocument,
    id: NativeNodeId,
) -> bool {
    document
        .raw_node(id)
        .and_then(NativeNode::namespace_uri)
        .is_some_and(|namespace| namespace != HTML_NAMESPACE_URI)
}

#[cfg(test)]
pub(super) fn process_html_any_other_end_tag(
    document: &NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    name: &str,
) -> Result<(), NativeEngineError> {
    for index in (1..stack.len()).rev() {
        let id = stack[index];
        let Some(node) = document.raw_node(id) else {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "HTML end-tag recovery reached an unknown open element".into(),
            });
        };
        if node.namespace_uri() == Some(HTML_NAMESPACE_URI)
            && node
                .element_name()
                .is_some_and(|node_name| node_name.eq_ignore_ascii_case(name))
        {
            stack.truncate(index);
            return Ok(());
        }
        if is_special_tree_builder_element(document, id) {
            return Ok(());
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(super) enum ForeignContentEndTagDisposition {
    ReprocessAsHtml,
    Breakout,
    Consumed,
}

#[cfg(test)]
pub(super) fn prepare_foreign_content_end_tag(
    document: &NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    name: &str,
    active_formatting: &mut ActiveFormattingList,
    fragment_html_context: bool,
) -> Result<ForeignContentEndTagDisposition, NativeEngineError> {
    let Some(current) = stack.last().copied() else {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: "foreign-content end-tag dispatch lost the open-elements stack".into(),
        });
    };
    if (fragment_html_context && stack.len() == 1)
        || !parser_uses_foreign_end_tag_rules(document, current)
    {
        return Ok(ForeignContentEndTagDisposition::ReprocessAsHtml);
    }

    if matches!(name, "br" | "p") {
        let stack_before = stack.clone();
        while stack.len() > 1 {
            let current = *stack.last().expect("non-empty open-elements stack");
            let Some(node) = document.raw_node(current) else {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "foreign-content end-tag breakout reached an unknown node".into(),
                });
            };
            if node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                || node_is_html_integration_point(node)
                || node_is_mathml_text_integration_point(node)
            {
                break;
            }
            stack.pop();
        }
        clear_markers_for_popped_elements(document, &stack_before, stack, active_formatting);
        return Ok(ForeignContentEndTagDisposition::Breakout);
    }

    let mut index = stack.len() - 1;
    loop {
        // The root is the fragment-context sentinel (or the document root); it
        // is never popped by foreign-content end-tag matching.
        if index == 0 {
            return Ok(ForeignContentEndTagDisposition::Consumed);
        }
        let node_id = stack[index];
        let Some(node) = document.raw_node(node_id) else {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "foreign-content end-tag dispatch reached an unknown node".into(),
            });
        };
        if node
            .element_name()
            .is_some_and(|node_name| node_name.eq_ignore_ascii_case(name))
        {
            let stack_before = stack.clone();
            stack.truncate(index);
            clear_markers_for_popped_elements(document, &stack_before, stack, active_formatting);
            return Ok(ForeignContentEndTagDisposition::Consumed);
        }

        index -= 1;
        let ancestor = stack[index];
        let Some(ancestor_node) = document.raw_node(ancestor) else {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "foreign-content end-tag dispatch reached an unknown ancestor".into(),
            });
        };
        if ancestor_node.namespace_uri() == Some(HTML_NAMESPACE_URI) {
            return Ok(ForeignContentEndTagDisposition::ReprocessAsHtml);
        }
    }
}

#[cfg(test)]
pub(super) fn insert_html_end_tag_recovery_element(
    document: &mut NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    fragment_root: Option<NativeNodeId>,
    force_html_context: bool,
    name: &str,
    push_to_stack: bool,
) -> Result<(), NativeEngineError> {
    let foster_location = html_table_foster_location(document, stack, fragment_root)?;
    let parent = foster_location
        .map(|(parent, _)| parent)
        .or_else(|| stack.last().copied())
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "end-tag recovery lost its HTML insertion parent".into(),
        })?;
    let current_depth = document.element_depth(parent);
    if current_depth >= document.max_dom_depth {
        return Err(NativeEngineError::limit(
            "DOM depth",
            document.max_dom_depth,
            current_depth.saturating_add(1),
        ));
    }
    let kind = NativeNodeKind::Element {
        name: name.to_owned(),
        attributes: BTreeMap::new(),
    };
    let id = match foster_location {
        Some((parent, Some(before))) => {
            document.add_node_before(parent, before, kind, document.max_nodes)?
        }
        Some((parent, None)) => document.add_node(parent, kind, document.max_nodes)?,
        None => document.add_node(parent, kind, document.max_nodes)?,
    };
    document.assign_parsed_namespace_to_node_with_html_context(
        id,
        parent,
        force_html_context && fragment_root == Some(parent),
    )?;
    if push_to_stack {
        stack.push(id);
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn node_is_html_integration_point(node: &NativeNode) -> bool {
    match (node.namespace_uri(), node.element_name()) {
        (Some(SVG_NAMESPACE_URI), Some("foreignobject" | "desc" | "title")) => true,
        (Some(MATHML_NAMESPACE_URI), Some("annotation-xml")) => {
            node.attribute("encoding").is_some_and(|encoding| {
                encoding.eq_ignore_ascii_case("text/html")
                    || encoding.eq_ignore_ascii_case("application/xhtml+xml")
            })
        }
        _ => false,
    }
}

#[cfg(test)]
pub(super) fn node_is_mathml_text_integration_point(node: &NativeNode) -> bool {
    node.namespace_uri() == Some(MATHML_NAMESPACE_URI)
        && matches!(
            node.element_name(),
            Some("mi" | "mo" | "mn" | "ms" | "mtext")
        )
}

#[cfg(test)]
pub(super) fn parsed_child_namespace(parent: &NativeNode, child_name: &str) -> &'static str {
    let parent_namespace = parent.namespace_uri().unwrap_or(HTML_NAMESPACE_URI);
    let parent_is_mathml_annotation_xml =
        parent_namespace == MATHML_NAMESPACE_URI && parent.element_name() == Some("annotation-xml");
    let uses_html_rules = parent_namespace == HTML_NAMESPACE_URI
        || node_is_html_integration_point(parent)
        || (node_is_mathml_text_integration_point(parent)
            && !matches!(child_name, "mglyph" | "malignmark"))
        || (parent_is_mathml_annotation_xml && child_name == "svg");

    if !uses_html_rules {
        return match parent_namespace {
            SVG_NAMESPACE_URI => SVG_NAMESPACE_URI,
            MATHML_NAMESPACE_URI => MATHML_NAMESPACE_URI,
            _ => HTML_NAMESPACE_URI,
        };
    }
    parsed_html_child_namespace(child_name)
}

#[cfg(test)]
pub(super) fn parsed_html_child_namespace(child_name: &str) -> &'static str {
    match child_name {
        "svg" => SVG_NAMESPACE_URI,
        "math" => MATHML_NAMESPACE_URI,
        _ => HTML_NAMESPACE_URI,
    }
}

#[cfg(test)]
pub(super) fn is_html_scope_boundary(document: &NativeDocument, id: NativeNodeId) -> bool {
    let Some(node) = document.raw_node(id) else {
        return true;
    };
    match node.namespace_uri() {
        Some(HTML_NAMESPACE_URI) => node.element_name().is_some_and(|name| {
            matches!(
                name,
                "applet"
                    | "caption"
                    | "html"
                    | "table"
                    | "td"
                    | "th"
                    | "marquee"
                    | "object"
                    | "select"
                    | "template"
            )
        }),
        Some(MATHML_NAMESPACE_URI) => node.element_name().is_some_and(|name| {
            matches!(name, "mi" | "mo" | "mn" | "ms" | "mtext" | "annotation-xml")
        }),
        Some(SVG_NAMESPACE_URI) => node
            .element_name()
            .is_some_and(|name| matches!(name, "foreignobject" | "desc" | "title")),
        _ => false,
    }
}

#[cfg(test)]
pub(super) fn html_element_is_in_scope(
    document: &NativeDocument,
    stack: &[NativeNodeId],
    target: NativeNodeId,
    fragment_root: Option<NativeNodeId>,
) -> bool {
    for id in stack.iter().rev().copied() {
        if Some(id) == fragment_root {
            return false;
        }
        if id == target {
            return true;
        }
        if is_html_scope_boundary(document, id) {
            return false;
        }
    }
    false
}

#[cfg(test)]
pub(super) fn html_paragraph_is_in_button_scope(
    document: &NativeDocument,
    stack: &[NativeNodeId],
    fragment_root: Option<NativeNodeId>,
) -> bool {
    for id in stack.iter().rev().copied() {
        if Some(id) == fragment_root {
            return false;
        }
        let Some(node) = document.raw_node(id) else {
            return false;
        };
        if node.namespace_uri() == Some(HTML_NAMESPACE_URI) && node.element_name() == Some("p") {
            return true;
        }
        if is_html_scope_boundary(document, id)
            || (node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                && node.element_name() == Some("button"))
        {
            return false;
        }
    }
    false
}

#[cfg(test)]
pub(super) fn parser_subtree_element_depth(document: &NativeDocument, root: NativeNodeId) -> usize {
    let mut maximum = 0;
    let mut pending = vec![(root, 0usize)];
    while let Some((id, parent_depth)) = pending.pop() {
        let Some(node) = document.raw_node(id) else {
            continue;
        };
        let depth = parent_depth + usize::from(node.element_name().is_some());
        maximum = maximum.max(depth);
        pending.extend(node.children().iter().copied().map(|child| (child, depth)));
    }
    maximum
}

#[cfg(test)]
pub(super) fn move_parser_node(
    document: &mut NativeDocument,
    parent: NativeNodeId,
    child: NativeNodeId,
    before: Option<NativeNodeId>,
    reason: &str,
) -> Result<(), NativeEngineError> {
    if document.raw_node(parent).is_none() || document.raw_node(child).is_none() {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: "HTML tree builder move references an unknown node".into(),
        });
    }
    if before.is_some_and(|before| {
        document.raw_node(before).and_then(NativeNode::parent) != Some(parent)
    }) {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: "HTML tree builder insertion reference is not a child of its parent".into(),
        });
    }
    if before == Some(child) {
        return Ok(());
    }
    let mut ancestor = Some(parent);
    while let Some(id) = ancestor {
        if id == child {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: format!(
                    "HTML tree builder {reason}: move of {} ({child:?}) beneath {} ({parent:?}) would create a DOM cycle",
                    document
                        .raw_node(child)
                        .and_then(NativeNode::element_name)
                        .unwrap_or("non-element"),
                    document
                        .raw_node(parent)
                        .and_then(NativeNode::element_name)
                        .unwrap_or("non-element"),
                ),
            });
        }
        ancestor = document.raw_node(id).and_then(NativeNode::parent);
    }
    let new_depth = document
        .element_depth(parent)
        .saturating_add(parser_subtree_element_depth(document, child));
    if new_depth > document.max_dom_depth {
        return Err(NativeEngineError::limit(
            "DOM depth",
            document.max_dom_depth,
            new_depth,
        ));
    }
    let old_parent = document.raw_node(child).and_then(NativeNode::parent);
    if let Some(old_parent) = old_parent {
        let old_parent_node = document
            .raw_node_mut(old_parent)
            .ok_or(NativeEngineError::DetachedTarget)?;
        old_parent_node
            .children
            .retain(|candidate| *candidate != child);
    }
    let parent_node = document
        .raw_node_mut(parent)
        .ok_or(NativeEngineError::DetachedTarget)?;
    let insertion_index = before.and_then(|before| {
        parent_node
            .children
            .iter()
            .position(|candidate| *candidate == before)
    });
    if before.is_some() && insertion_index.is_none() {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: "HTML tree builder lost its insertion reference".into(),
        });
    }
    match insertion_index {
        Some(index) => parent_node.children.insert(index, child),
        None => parent_node.children.push(child),
    }
    document
        .raw_node_mut(child)
        .ok_or(NativeEngineError::DetachedTarget)?
        .parent = Some(parent);
    Ok(())
}

#[cfg(test)]
pub(super) fn insert_parser_formatting_clone(
    document: &mut NativeDocument,
    stack: &[NativeNodeId],
    fallback_parent: NativeNodeId,
    fragment_root: Option<NativeNodeId>,
    name: &str,
    attributes: BTreeMap<String, String>,
) -> Result<NativeNodeId, NativeEngineError> {
    let foster_location = html_table_foster_location(document, stack, fragment_root)?;
    let parent = foster_location
        .map(|(parent, _)| parent)
        .unwrap_or(fallback_parent);
    let depth = document.element_depth(parent);
    if depth >= document.max_dom_depth {
        return Err(NativeEngineError::limit(
            "DOM depth",
            document.max_dom_depth,
            depth.saturating_add(1),
        ));
    }
    let kind = NativeNodeKind::Element {
        name: name.to_owned(),
        attributes,
    };
    let id = match foster_location {
        Some((parent, Some(before))) => {
            document.add_node_before(parent, before, kind, document.max_nodes)?
        }
        Some((parent, None)) => document.add_node(parent, kind, document.max_nodes)?,
        None => document.add_node(parent, kind, document.max_nodes)?,
    };
    document
        .raw_node_mut(id)
        .ok_or(NativeEngineError::DetachedTarget)?
        .state
        .namespace_uri = Some(HTML_NAMESPACE_URI.to_owned());
    Ok(id)
}

#[cfg(test)]
pub(super) fn reconstruct_active_formatting_elements(
    document: &mut NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    active: &mut ActiveFormattingList,
    fragment_root: Option<NativeNodeId>,
    force_html_fragment_context: bool,
) -> Result<(), NativeEngineError> {
    let current_is_forced_html_fragment_root =
        force_html_fragment_context && stack.last().copied() == fragment_root;
    if !current_is_forced_html_fragment_root
        && stack
            .last()
            .and_then(|id| document.raw_node(*id))
            .is_some_and(|node| {
                node.namespace_uri().is_some_and(|namespace| {
                    namespace != HTML_NAMESPACE_URI
                        && !(namespace == SVG_NAMESPACE_URI
                            && matches!(
                                node.element_name(),
                                Some("foreignobject" | "desc" | "title")
                            ))
                        && !(namespace == MATHML_NAMESPACE_URI
                            && (matches!(
                                node.element_name(),
                                Some("mi" | "mo" | "mn" | "ms" | "mtext")
                            ) || (node.element_name() == Some("annotation-xml")
                                && node.attribute("encoding").is_some_and(|value| {
                                    value.eq_ignore_ascii_case("text/html")
                                        || value.eq_ignore_ascii_case("application/xhtml+xml")
                                }))))
                })
            })
    {
        return Ok(());
    }
    let Some(last_index) = active.len().checked_sub(1) else {
        return Ok(());
    };
    let ActiveFormattingEntry::Element(last_id) = active[last_index] else {
        return Ok(());
    };
    if stack.contains(&last_id) {
        return Ok(());
    }
    let mut first_index = last_index;
    while first_index > 0 {
        match active[first_index - 1] {
            ActiveFormattingEntry::Marker => break,
            ActiveFormattingEntry::Element(id) if stack.contains(&id) => break,
            ActiveFormattingEntry::Element(_) => first_index -= 1,
        }
    }
    for entry in active.iter_mut().skip(first_index) {
        let ActiveFormattingEntry::Element(old_id) = entry else {
            continue;
        };
        let (name, attributes) = match document.raw_node(*old_id).map(NativeNode::kind) {
            Some(NativeNodeKind::Element { name, attributes }) => {
                (name.clone(), attributes.clone())
            }
            _ => {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "active formatting list references a non-element".into(),
                });
            }
        };
        let parent = stack
            .last()
            .copied()
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "active formatting reconstruction lost the open-elements stack".into(),
            })?;
        let new_id = insert_parser_formatting_clone(
            document,
            stack,
            parent,
            fragment_root,
            &name,
            attributes,
        )?;
        stack.push(new_id);
        *entry = ActiveFormattingEntry::Element(new_id);
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn adopt_active_formatting_element(
    document: &mut NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    active: &mut ActiveFormattingList,
    fragment_root: Option<NativeNodeId>,
    subject: &str,
) -> Result<bool, NativeEngineError> {
    if stack
        .last()
        .and_then(|id| document.raw_node(*id))
        .is_some_and(|node| {
            node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                && node.element_name() == Some(subject)
                && active_formatting_entry_index(document, active, subject).is_none()
        })
    {
        stack.pop();
        return Ok(true);
    }

    for _ in 0..8 {
        let Some(formatting_entry_index) = active_formatting_entry_index(document, active, subject)
        else {
            return Ok(false);
        };
        let ActiveFormattingEntry::Element(formatting_id) = active[formatting_entry_index] else {
            unreachable!("formatting lookup never selects a marker")
        };
        let Some(formatting_stack_index) = stack.iter().position(|id| *id == formatting_id) else {
            active.remove(formatting_entry_index);
            return Ok(true);
        };
        if !html_element_is_in_scope(document, stack, formatting_id, fragment_root) {
            return Ok(true);
        }
        let Some(furthest_stack_index) = (formatting_stack_index + 1..stack.len())
            .rev()
            .find(|index| is_special_tree_builder_element(document, stack[*index]))
        else {
            stack.truncate(formatting_stack_index);
            active.remove(formatting_entry_index);
            return Ok(true);
        };
        let furthest_block = stack[furthest_stack_index];
        let common_ancestor =
            *stack
                .get(formatting_stack_index.checked_sub(1).ok_or_else(|| {
                    NativeEngineError::Parse {
                        offset: 0,
                        reason: "adoption agency formatting element has no common ancestor".into(),
                    }
                })?)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "adoption agency formatting element has no common ancestor".into(),
                })?;
        let mut bookmark = formatting_entry_index;
        let mut node_id = furthest_block;
        let mut removed_node_parent = None;
        let mut last_node = furthest_block;
        let mut inner_loop_counter = 0usize;

        loop {
            inner_loop_counter += 1;
            let candidate_index =
                if let Some(node_stack_index) = stack.iter().position(|id| *id == node_id) {
                    let Some(index) = node_stack_index.checked_sub(1) else {
                        return Err(NativeEngineError::Parse {
                            offset: 0,
                            reason: "adoption agency reached the parser root".into(),
                        });
                    };
                    index
                } else if let Some(parent_of_removed_node) = removed_node_parent.take() {
                    stack
                        .iter()
                        .position(|id| *id == parent_of_removed_node)
                        .ok_or_else(|| NativeEngineError::Parse {
                            offset: 0,
                            reason: "adoption agency lost the parent of a removed node".into(),
                        })?
                } else {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "adoption agency lost a node from the open-elements stack".into(),
                    });
                };
            let candidate = stack[candidate_index];
            if candidate == formatting_id {
                break;
            }
            if inner_loop_counter > 3
                && let Some(index) = active
                    .iter()
                    .position(|entry| *entry == ActiveFormattingEntry::Element(candidate))
            {
                active.remove(index);
                if index < bookmark {
                    bookmark -= 1;
                }
            }
            let Some(active_index) = active
                .iter()
                .position(|entry| *entry == ActiveFormattingEntry::Element(candidate))
            else {
                removed_node_parent = candidate_index.checked_sub(1).map(|index| stack[index]);
                node_id = candidate;
                stack.remove(candidate_index);
                continue;
            };
            let (name, attributes) = match document.raw_node(candidate).map(NativeNode::kind) {
                Some(NativeNodeKind::Element { name, attributes }) => {
                    (name.clone(), attributes.clone())
                }
                _ => {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "adoption agency active entry is not an element".into(),
                    });
                }
            };
            let clone_id = insert_parser_formatting_clone(
                document,
                stack,
                common_ancestor,
                fragment_root,
                &name,
                attributes,
            )?;
            active[active_index] = ActiveFormattingEntry::Element(clone_id);
            let stack_index = stack
                .iter()
                .position(|id| *id == candidate)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "adoption agency lost a formatting node".into(),
                })?;
            stack[stack_index] = clone_id;
            if last_node == furthest_block {
                bookmark = active_index + 1;
            }
            move_parser_node(document, clone_id, last_node, None, "adoption inner loop")?;
            last_node = clone_id;
            node_id = clone_id;
            removed_node_parent = None;
        }

        let insertion = html_table_foster_location(document, stack, fragment_root)?;
        let (target, before) = insertion.unwrap_or((common_ancestor, None));
        move_parser_node(
            document,
            target,
            last_node,
            before,
            "adoption appropriate insertion location",
        )?;

        let furthest_children = document
            .raw_node(furthest_block)
            .ok_or(NativeEngineError::DetachedTarget)?
            .children
            .clone();
        let (formatting_name, formatting_attributes) =
            match document.raw_node(formatting_id).map(NativeNode::kind) {
                Some(NativeNodeKind::Element { name, attributes }) => {
                    (name.clone(), attributes.clone())
                }
                _ => {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "adoption agency formatting node is not an element".into(),
                    });
                }
            };
        let replacement = document.add_node(
            furthest_block,
            NativeNodeKind::Element {
                name: formatting_name,
                attributes: formatting_attributes,
            },
            document.max_nodes,
        )?;
        document
            .raw_node_mut(replacement)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state
            .namespace_uri = Some(HTML_NAMESPACE_URI.to_owned());
        for child in furthest_children {
            move_parser_node(
                document,
                replacement,
                child,
                None,
                "furthest-block child reparenting",
            )?;
        }

        if let Some(index) = active
            .iter()
            .position(|entry| *entry == ActiveFormattingEntry::Element(formatting_id))
        {
            let insertion_index = bookmark.saturating_sub(usize::from(index < bookmark));
            active.remove(index);
            active.insert(
                insertion_index.min(active.len()),
                ActiveFormattingEntry::Element(replacement),
            );
        }
        let formatting_stack_index = stack
            .iter()
            .position(|id| *id == formatting_id)
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "adoption agency formatting node left the open-elements stack".into(),
            })?;
        stack.remove(formatting_stack_index);
        let furthest_stack_index = stack
            .iter()
            .position(|id| *id == furthest_block)
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "adoption agency furthest block left the open-elements stack".into(),
            })?;
        stack.insert(furthest_stack_index + 1, replacement);
    }
    Ok(true)
}

#[cfg(test)]
pub(super) fn consume_formatting_fallback_end_tag(
    document: &NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    fragment_root: Option<NativeNodeId>,
    subject: &str,
) -> bool {
    if stack
        .last()
        .and_then(|id| document.raw_node(*id))
        .is_some_and(|node| {
            node.namespace_uri().is_some_and(|namespace| {
                namespace != HTML_NAMESPACE_URI
                    && !(namespace == SVG_NAMESPACE_URI
                        && matches!(
                            node.element_name(),
                            Some("foreignobject" | "desc" | "title")
                        ))
                    && !(namespace == MATHML_NAMESPACE_URI
                        && (matches!(
                            node.element_name(),
                            Some("mi" | "mo" | "mn" | "ms" | "mtext")
                        ) || (node.element_name() == Some("annotation-xml")
                            && node.attribute("encoding").is_some_and(|value| {
                                value.eq_ignore_ascii_case("text/html")
                                    || value.eq_ignore_ascii_case("application/xhtml+xml")
                            }))))
            })
        })
    {
        return false;
    }
    for index in (0..stack.len()).rev() {
        let id = stack[index];
        if Some(id) == fragment_root {
            return true;
        }
        let Some(node) = document.raw_node(id) else {
            return true;
        };
        if node.namespace_uri() == Some(HTML_NAMESPACE_URI) && node.element_name() == Some(subject)
        {
            stack.truncate(index);
            return true;
        }
        if is_special_tree_builder_element(document, id) {
            return true;
        }
    }
    true
}

#[cfg(test)]
pub(super) fn html_table_foster_location(
    document: &NativeDocument,
    stack: &[NativeNodeId],
    fragment_root: Option<NativeNodeId>,
) -> Result<Option<(NativeNodeId, Option<NativeNodeId>)>, NativeEngineError> {
    let Some(current) = stack.last().and_then(|id| document.node(*id)) else {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node".into(),
        });
    };
    if current.namespace_uri() != Some(HTML_NAMESPACE_URI)
        || !matches!(
            current.element_name(),
            Some("table" | "tbody" | "tfoot" | "thead" | "tr")
        )
    {
        return Ok(None);
    }
    let table = stack.iter().rev().copied().find(|id| {
        Some(*id) != fragment_root
            && document.node(*id).is_some_and(|node| {
                node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                    && node.element_name() == Some("table")
            })
    });
    let Some(table) = table else {
        if let Some(fragment_root) = fragment_root {
            return Ok(Some((fragment_root, None)));
        }
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: "HTML table insertion context has no open table".into(),
        });
    };
    let parent = document
        .node(table)
        .and_then(NativeNode::parent)
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "open HTML table has no insertion parent".into(),
        })?;
    Ok(Some((parent, Some(table))))
}

#[cfg(test)]
pub(super) fn prepare_html_table_start_token(
    document: &NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    fragment_root: Option<NativeNodeId>,
) -> Result<bool, NativeEngineError> {
    let current = stack
        .last()
        .and_then(|id| document.node(*id))
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node while processing a table start".into(),
        })?;
    if current.namespace_uri() != Some(HTML_NAMESPACE_URI) {
        return Ok(true);
    }

    loop {
        let mut table_index = None;
        for index in (0..stack.len()).rev() {
            let id = stack[index];
            let node = document.node(id).ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder table scope references an unknown node".into(),
            })?;
            if node.namespace_uri() != Some(HTML_NAMESPACE_URI) {
                if node.namespace_uri() == Some(SVG_NAMESPACE_URI)
                    && node.element_name() == Some("foreignobject")
                {
                    return Ok(true);
                }
                continue;
            }
            match node.element_name() {
                Some("template") => return Ok(true),
                Some("table") if Some(id) != fragment_root => {
                    table_index = Some(index);
                    break;
                }
                _ => {}
            }
        }

        if let Some(index) = table_index {
            let table_mode_boundary = stack[index + 1..].iter().any(|id| {
                document.node(*id).is_some_and(|node| {
                    (node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                        && matches!(
                            node.element_name(),
                            Some("td" | "th" | "caption" | "template")
                        ))
                        || (node.namespace_uri() == Some(SVG_NAMESPACE_URI)
                            && node.element_name() == Some("foreignobject"))
                })
            });
            if table_mode_boundary {
                return Ok(true);
            }

            // Reprocessing the token after a table pop can expose another
            // active table-mode scope, so remove each such table before the
            // caller inserts this same start tag.
            stack.truncate(index);
            continue;
        }

        let Some(fragment_root) = fragment_root else {
            return Ok(true);
        };
        let Some(root_index) = stack.iter().position(|id| *id == fragment_root) else {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "fragment parser lost its context element".into(),
            });
        };
        let context_establishes_table_mode = document.node(fragment_root).is_some_and(|node| {
            node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                && matches!(
                    node.element_name(),
                    Some("table" | "colgroup" | "tbody" | "tfoot" | "thead" | "tr")
                )
        });
        let entered_cell_or_caption = stack[root_index + 1..].iter().any(|id| {
            document.node(*id).is_some_and(|node| {
                node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                    && matches!(node.element_name(), Some("td" | "th" | "caption"))
            })
        });
        if context_establishes_table_mode && !entered_cell_or_caption {
            // A fragment's context element selects the insertion mode but is
            // not an element on the parser's open-elements stack.
            return Ok(false);
        }
        return Ok(true);
    }
}

#[cfg(test)]
pub(super) fn consume_html_table_end_token(
    document: &NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    fragment_root: Option<NativeNodeId>,
) -> Result<bool, NativeEngineError> {
    let Some(current_id) = stack.last().copied() else {
        return Ok(false);
    };
    let current = document
        .node(current_id)
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node while processing a table end".into(),
        })?;
    if current.namespace_uri() != Some(HTML_NAMESPACE_URI) {
        return Ok(false);
    }

    for index in (0..stack.len()).rev() {
        let id = stack[index];
        if Some(id) == fragment_root {
            continue;
        }
        let node = document.node(id).ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder table scope references an unknown node".into(),
        })?;
        if node.namespace_uri() != Some(HTML_NAMESPACE_URI) {
            continue;
        }
        match node.element_name() {
            Some("table") => {
                stack.truncate(index);
                return Ok(true);
            }
            Some("html" | "template") => return Ok(true),
            _ => {}
        }
    }

    // In an HTML parsing context, an out-of-scope table end tag is ignored.
    Ok(true)
}

#[cfg(test)]
pub(super) fn consume_html_table_structure_end_token(
    document: &NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    fragment_root: Option<NativeNodeId>,
    target_name: &str,
) -> Result<bool, NativeEngineError> {
    let Some(current_id) = stack.last().copied() else {
        return Ok(false);
    };
    let current = document
        .node(current_id)
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node while processing a table-structure end"
                .into(),
        })?;
    if current.namespace_uri() != Some(HTML_NAMESPACE_URI) {
        return Ok(false);
    }

    for index in (0..stack.len()).rev() {
        let id = stack[index];
        if Some(id) == fragment_root {
            continue;
        }
        let node = document.node(id).ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder table scope references an unknown node".into(),
        })?;
        if node.namespace_uri() != Some(HTML_NAMESPACE_URI) {
            continue;
        }
        match node.element_name() {
            Some(name) if name == target_name => {
                stack.truncate(index);
                return Ok(true);
            }
            Some("html" | "table" | "template") => return Ok(true),
            _ => {}
        }
    }

    // An unmatched table-structure end tag is ignored in an HTML context.
    Ok(true)
}

#[cfg(test)]
pub(super) fn consume_html_table_column_group_end_token(
    document: &NativeDocument,
    stack: &mut Vec<NativeNodeId>,
    fragment_root: Option<NativeNodeId>,
    target_name: &str,
) -> Result<bool, NativeEngineError> {
    if matches!(target_name, "br" | "p")
        && stack.len() == 1
        && fragment_root == stack.first().copied()
        && fragment_root
            .and_then(|root| document.node(root))
            .is_some_and(|node| {
                node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                    && node.element_name() == Some("colgroup")
            })
    {
        return Ok(true);
    }
    if !matches!(
        target_name,
        "body"
            | "br"
            | "caption"
            | "col"
            | "colgroup"
            | "html"
            | "p"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
    ) {
        return Ok(false);
    }
    let Some(current_id) = stack.last().copied() else {
        return Ok(false);
    };
    let current = document
        .node(current_id)
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node in a column-group end tag".into(),
        })?;
    if current.namespace_uri() != Some(HTML_NAMESPACE_URI) {
        return Ok(false);
    }

    let mut column_group_index = None;
    for index in (0..stack.len()).rev() {
        let id = stack[index];
        if Some(id) == fragment_root {
            return Ok(false);
        }
        let node = document.node(id).ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder column-group scope references an unknown node".into(),
        })?;
        if node.namespace_uri() != Some(HTML_NAMESPACE_URI) {
            continue;
        }
        match node.element_name() {
            Some("td" | "th" | "tr" | "tbody" | "tfoot" | "thead" | "caption") => {
                return Ok(false);
            }
            Some("colgroup") if column_group_index.is_none() => {
                column_group_index = Some(index);
            }
            Some("table") => {
                let Some(index) = column_group_index else {
                    return Ok(false);
                };
                match target_name {
                    "br" | "p" => return Ok(true),
                    "col" => return Ok(true),
                    "colgroup" => {
                        if index + 1 == stack.len() {
                            stack.truncate(index);
                        }
                        return Ok(true);
                    }
                    _ => {
                        // In column-group mode, other table end tags pop the
                        // current group, then reprocess into table mode.
                        if index + 1 == stack.len() {
                            stack.truncate(index);
                        }
                        return Ok(true);
                    }
                }
            }
            Some("html" | "template") => return Ok(false),
            _ => {}
        }
    }
    Ok(false)
}

#[cfg(test)]
pub(super) fn consume_html_table_cell_ignored_end_token(
    document: &NativeDocument,
    stack: &[NativeNodeId],
    fragment_root: Option<NativeNodeId>,
    target_name: &str,
) -> Result<bool, NativeEngineError> {
    if !matches!(
        target_name,
        "body" | "caption" | "col" | "colgroup" | "html"
    ) {
        return Ok(false);
    }
    let Some(current_id) = stack.last().copied() else {
        return Ok(false);
    };
    let current = document
        .node(current_id)
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node while processing an in-cell end tag".into(),
        })?;
    if current.namespace_uri() != Some(HTML_NAMESPACE_URI) {
        return Ok(false);
    }

    let mut cell_in_scope = false;
    for id in stack.iter().rev().copied() {
        if Some(id) == fragment_root {
            return Ok(false);
        }
        let node = document.node(id).ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder in-cell scope references an unknown node".into(),
        })?;
        if node.namespace_uri() != Some(HTML_NAMESPACE_URI) {
            continue;
        }
        match node.element_name() {
            Some("td" | "th") => cell_in_scope = true,
            Some("table") => return Ok(cell_in_scope),
            Some("html" | "template") => return Ok(false),
            _ => {}
        }
    }
    Ok(false)
}

#[cfg(test)]
pub(super) fn consume_html_table_row_ignored_end_token(
    document: &NativeDocument,
    stack: &[NativeNodeId],
    fragment_root: Option<NativeNodeId>,
    target_name: &str,
) -> Result<bool, NativeEngineError> {
    if !matches!(
        target_name,
        "body" | "caption" | "col" | "colgroup" | "html"
    ) {
        return Ok(false);
    }
    let Some(current_id) = stack.last().copied() else {
        return Ok(false);
    };
    let current = document
        .node(current_id)
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node while processing a row end tag".into(),
        })?;
    if current.namespace_uri() != Some(HTML_NAMESPACE_URI) {
        return Ok(false);
    }

    let mut row_context_in_scope = false;
    for id in stack.iter().rev().copied() {
        if Some(id) == fragment_root {
            return Ok(false);
        }
        let node = document.node(id).ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder row scope references an unknown node".into(),
        })?;
        if node.namespace_uri() != Some(HTML_NAMESPACE_URI) {
            continue;
        }
        match node.element_name() {
            Some("td" | "th" | "caption" | "colgroup") => return Ok(false),
            Some("tr" | "tbody" | "tfoot" | "thead") => row_context_in_scope = true,
            Some("table") => return Ok(row_context_in_scope),
            Some("html" | "template") => return Ok(false),
            _ => {}
        }
    }
    Ok(false)
}

#[cfg(test)]
pub(super) fn consume_html_table_caption_ignored_end_token(
    document: &NativeDocument,
    stack: &[NativeNodeId],
    fragment_root: Option<NativeNodeId>,
    target_name: &str,
) -> Result<bool, NativeEngineError> {
    if !matches!(
        target_name,
        "body" | "col" | "colgroup" | "html" | "tbody" | "td" | "tfoot" | "th" | "thead" | "tr"
    ) {
        return Ok(false);
    }
    let Some(current_id) = stack.last().copied() else {
        return Ok(false);
    };
    let current = document
        .node(current_id)
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node in a caption end tag".into(),
        })?;
    if current.namespace_uri() != Some(HTML_NAMESPACE_URI) {
        return Ok(false);
    }

    let mut caption_in_scope = false;
    for id in stack.iter().rev().copied() {
        if Some(id) == fragment_root {
            return Ok(false);
        }
        let node = document.node(id).ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder caption scope references an unknown node".into(),
        })?;
        if node.namespace_uri() != Some(HTML_NAMESPACE_URI) {
            continue;
        }
        match node.element_name() {
            Some("td" | "th" | "tr" | "tbody" | "tfoot" | "thead" | "colgroup") => {
                return Ok(false);
            }
            Some("caption") => caption_in_scope = true,
            Some("table") => return Ok(caption_in_scope),
            Some("html" | "template") => return Ok(false),
            _ => {}
        }
    }
    Ok(false)
}

#[cfg(test)]
pub(super) fn consume_html_table_mode_ignored_end_token(
    document: &NativeDocument,
    stack: &[NativeNodeId],
    fragment_root: Option<NativeNodeId>,
    target_name: &str,
) -> Result<bool, NativeEngineError> {
    if !matches!(
        target_name,
        "body"
            | "caption"
            | "col"
            | "colgroup"
            | "html"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
    ) {
        return Ok(false);
    }
    let Some(current_id) = stack.last().copied() else {
        return Ok(false);
    };
    let current = document
        .node(current_id)
        .ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder lost its current node in a table-mode end tag".into(),
        })?;
    if current.namespace_uri() != Some(HTML_NAMESPACE_URI) {
        return Ok(false);
    }

    for id in stack.iter().rev().copied() {
        if Some(id) == fragment_root {
            return Ok(false);
        }
        let node = document.node(id).ok_or_else(|| NativeEngineError::Parse {
            offset: 0,
            reason: "tree builder table-mode scope references an unknown node".into(),
        })?;
        if node.namespace_uri() != Some(HTML_NAMESPACE_URI) {
            continue;
        }
        match node.element_name() {
            Some("td" | "th" | "tr" | "tbody" | "tfoot" | "thead" | "caption" | "colgroup") => {
                return Ok(false);
            }
            Some("table") => return Ok(true),
            Some("html" | "template") => return Ok(false),
            _ => {}
        }
    }
    Ok(false)
}

#[cfg(test)]
pub(super) fn replace_html_nulls<'a>(value: &'a str, replacement: &str) -> Cow<'a, str> {
    if value.contains('\0') {
        Cow::Owned(value.replace('\0', replacement))
    } else {
        Cow::Borrowed(value)
    }
}

#[cfg(test)]
pub(super) fn decode_html_attribute_value(value: &str) -> String {
    let decoded = decode_entities(value);
    replace_html_nulls(&decoded, "\u{FFFD}").into_owned()
}

#[cfg(test)]
pub(super) fn decode_html_character_data(value: &str, foreign_content: bool) -> String {
    let literal_null_replacement = if foreign_content { "\u{FFFD}" } else { "" };
    let decoded = decode_entities(value);
    replace_html_nulls(&decoded, literal_null_replacement).into_owned()
}

#[cfg(test)]
pub(super) fn decode_entities(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        let Some(relative_ampersand) = value[cursor..].find('&') else {
            output.push_str(&value[cursor..]);
            break;
        };
        let ampersand = cursor + relative_ampersand;
        output.push_str(&value[cursor..ampersand]);
        let Some(relative_semicolon) = value[ampersand + 1..].find(';') else {
            output.push('&');
            cursor = ampersand + 1;
            continue;
        };
        let semicolon = ampersand + 1 + relative_semicolon;
        let entity = &value[ampersand + 1..semicolon];
        if let Some(decoded) = decode_entity(entity) {
            output.push(decoded);
            cursor = semicolon + 1;
        } else {
            output.push('&');
            cursor = ampersand + 1;
        }
    }
    output
}

#[cfg(test)]
pub(super) fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(|code_point| {
                    if code_point == 0 {
                        Some('\u{FFFD}')
                    } else {
                        char::from_u32(code_point)
                    }
                })
        }
        _ if entity.starts_with('#') => entity[1..].parse().ok().and_then(|code_point| {
            if code_point == 0 {
                Some('\u{FFFD}')
            } else {
                char::from_u32(code_point)
            }
        }),
        _ => None,
    }
}

pub(super) fn validate_script_attribute(name: &str) -> Result<String, NativeEngineError> {
    if name.is_empty() {
        return Err(NativeEngineError::invalid(
            "script attribute name",
            "must not be empty",
        ));
    }
    if name.len() > MAX_ATTRIBUTE_BYTES {
        return Err(NativeEngineError::limit(
            "script attribute name",
            MAX_ATTRIBUTE_BYTES,
            name.len(),
        ));
    }
    if name
        .bytes()
        .any(|byte| byte.is_ascii_whitespace() || matches!(byte, b'"' | b'\'' | b'<' | b'>' | b'='))
    {
        return Err(NativeEngineError::invalid(
            "script attribute name",
            "contains a forbidden character",
        ));
    }
    Ok(name.to_ascii_lowercase())
}

pub(super) fn validate_script_element_name(name: &str) -> Result<String, NativeEngineError> {
    if name.len() > MAX_ATTRIBUTE_BYTES {
        return Err(NativeEngineError::limit(
            "script element name",
            MAX_ATTRIBUTE_BYTES,
            name.len(),
        ));
    }
    let Some((normalized, end)) = read_name(name, 0) else {
        return Err(NativeEngineError::invalid(
            "script element name",
            "must start with an ASCII letter",
        ));
    };
    if end != name.len() {
        return Err(NativeEngineError::invalid(
            "script element name",
            "contains an unsupported character",
        ));
    }
    Ok(normalized)
}

fn read_name(raw: &str, start: usize) -> Option<(String, usize)> {
    let bytes = raw.as_bytes();
    if start >= bytes.len() || !bytes[start].is_ascii_alphabetic() {
        return None;
    }
    let mut end = start + 1;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'-' | b'_' | b':'))
    {
        end += 1;
    }
    Some((raw[start..end].to_ascii_lowercase(), end))
}

pub(super) fn validate_namespace_uri(value: &str) -> Result<String, NativeEngineError> {
    if value.len() > MAX_ATTRIBUTE_BYTES {
        return Err(NativeEngineError::limit(
            "element namespace URI",
            MAX_ATTRIBUTE_BYTES,
            value.len(),
        ));
    }
    if matches!(
        value,
        HTML_NAMESPACE_URI | SVG_NAMESPACE_URI | MATHML_NAMESPACE_URI
    ) {
        return Ok(value.to_owned());
    }
    Err(NativeEngineError::invalid(
        "element namespace URI",
        "is not a supported HTML, SVG, or MathML namespace",
    ))
}

pub(super) fn validate_attribute_namespace_uri(value: &str) -> Result<String, NativeEngineError> {
    if value.len() > MAX_ATTRIBUTE_BYTES {
        return Err(NativeEngineError::limit(
            "attribute namespace URI",
            MAX_ATTRIBUTE_BYTES,
            value.len(),
        ));
    }
    if matches!(
        value,
        HTML_NAMESPACE_URI
            | SVG_NAMESPACE_URI
            | MATHML_NAMESPACE_URI
            | XML_NAMESPACE_URI
            | XMLNS_NAMESPACE_URI
            | XLINK_NAMESPACE_URI
    ) {
        return Ok(value.to_owned());
    }
    Err(NativeEngineError::invalid(
        "attribute namespace URI",
        "is not a supported HTML, SVG, MathML, XML, XMLNS, or XLink namespace",
    ))
}

pub(super) fn inferred_attribute_namespace(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if lower == "xmlns" || lower.starts_with("xmlns:") {
        Some(XMLNS_NAMESPACE_URI)
    } else if lower.starts_with("xml:") {
        Some(XML_NAMESPACE_URI)
    } else if lower.starts_with("xlink:") {
        Some(XLINK_NAMESPACE_URI)
    } else {
        None
    }
}

#[cfg(test)]
pub(super) const SVG_ATTRIBUTE_NAME_ADJUSTMENTS: &[(&str, &str)] = &[
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

#[cfg(test)]
pub(super) fn adjust_parsed_attributes_for_namespace(node: &mut NativeNode) {
    let namespace = node
        .namespace_uri()
        .unwrap_or(HTML_NAMESPACE_URI)
        .to_owned();
    let NativeNodeKind::Element { attributes, .. } = &mut node.kind else {
        return;
    };
    let original_attributes = std::mem::take(attributes);
    let mut adjusted_attributes = BTreeMap::new();
    let mut attribute_namespaces = BTreeMap::new();
    for (name, value) in original_attributes {
        let adjusted_name = match (namespace.as_str(), name.as_str()) {
            (SVG_NAMESPACE_URI, name) => SVG_ATTRIBUTE_NAME_ADJUSTMENTS
                .iter()
                .find_map(|(token_name, adjusted)| (*token_name == name).then_some(*adjusted))
                .unwrap_or(name),
            (MATHML_NAMESPACE_URI, "definitionurl") => "definitionURL",
            _ => name.as_str(),
        };
        let attribute_namespace = match name.as_str() {
            "xlink:actuate" | "xlink:arcrole" | "xlink:href" | "xlink:role" | "xlink:show"
            | "xlink:title" | "xlink:type"
                if namespace.as_str() != HTML_NAMESPACE_URI =>
            {
                Some(XLINK_NAMESPACE_URI)
            }
            "xml:lang" | "xml:space" if namespace.as_str() != HTML_NAMESPACE_URI => {
                Some(XML_NAMESPACE_URI)
            }
            "xmlns" | "xmlns:xlink" if namespace.as_str() != HTML_NAMESPACE_URI => {
                Some(XMLNS_NAMESPACE_URI)
            }
            _ => None,
        };
        if adjusted_attributes.contains_key(adjusted_name) {
            continue;
        }
        if let Some(attribute_namespace) = attribute_namespace {
            attribute_namespaces.insert(adjusted_name.to_owned(), attribute_namespace.to_owned());
        }
        adjusted_attributes.insert(adjusted_name.to_owned(), value);
    }
    *attributes = adjusted_attributes;
    node.state.attribute_namespaces = attribute_namespaces;
}

pub(super) fn collapse_text(value: &str, max_bytes: usize) -> (String, bool) {
    let mut output = String::new();
    let mut truncated = false;
    append_collapsed_text(&mut output, value, max_bytes, &mut truncated);
    (output, truncated)
}

pub(super) fn encode_data_url_payload(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

pub(super) fn append_collapsed_text(
    output: &mut String,
    value: &str,
    max_bytes: usize,
    truncated: &mut bool,
) {
    let mut word = String::new();
    for character in value.chars() {
        if character.is_whitespace() {
            append_word(output, &word, max_bytes, truncated);
            word.clear();
            if *truncated {
                return;
            }
        } else {
            word.push(character);
        }
    }
    append_word(output, &word, max_bytes, truncated);
}

pub(super) fn append_word(output: &mut String, word: &str, max_bytes: usize, truncated: &mut bool) {
    if word.is_empty() || *truncated {
        return;
    }
    let separator = usize::from(!output.is_empty());
    if output.len().saturating_add(separator) >= max_bytes {
        *truncated = true;
        return;
    }
    let available = max_bytes - output.len() - separator;
    if word.len() > available {
        if separator == 1 {
            output.push(' ');
        }
        let mut used = 0;
        for character in word.chars() {
            let width = character.len_utf8();
            if used + width > available {
                break;
            }
            output.push(character);
            used += width;
        }
        *truncated = true;
        return;
    }
    if separator == 1 {
        output.push(' ');
    }
    output.push_str(word);
}

pub(super) fn escape_html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn escape_html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn append_bounded_markup(
    output: &mut String,
    value: &str,
    max_bytes: usize,
    truncated: &mut bool,
) {
    if *truncated {
        return;
    }
    let available = max_bytes.saturating_sub(output.len());
    if value.len() <= available {
        output.push_str(value);
        return;
    }
    let mut end = available.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    output.push_str(&value[..end]);
    *truncated = true;
}

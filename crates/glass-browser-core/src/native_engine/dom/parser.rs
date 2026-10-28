use super::super::html_parser::{HtmlParsedNode, parse_document_with_limits as parse_html5};
use super::*;

const HTML_USER_AGENT_STYLESHEET: &str = "body { margin: 8px; }";

pub(super) fn with_html_user_agent_stylesheet(mut sources: Vec<String>) -> Vec<String> {
    // The browser's initial body margin is a user-agent rule. Keep it first so
    // author stylesheets and inline declarations retain normal cascade order.
    sources.insert(0, HTML_USER_AGENT_STYLESHEET.into());
    sources
}

impl NativeDocument {
    /// Parse one bounded HTML document using html5ever's tree builder and a
    /// Glass-owned, resource-limited node sink, without browser user-agent
    /// presentation defaults. Browser navigation applies those defaults when
    /// it constructs the live document. This is not a full HTML conformance
    /// claim.
    pub fn parse(source: &str, limits: &NativeEngineLimits) -> Result<Self, NativeEngineError> {
        Self::parse_with_stylesheets_inline_policy_and_referrer_policy_with_ua_stylesheet(
            source,
            limits,
            &[],
            1,
            None,
            NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            false,
        )
    }

    #[cfg(test)]
    pub(crate) fn parse_with_generation(
        source: &str,
        limits: &NativeEngineLimits,
        generation: u32,
    ) -> Result<Self, NativeEngineError> {
        Self::parse_with_stylesheets_inline_policy_and_referrer_policy_with_ua_stylesheet(
            source,
            limits,
            &[],
            generation,
            None,
            NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            false,
        )
    }

    pub(crate) fn parse_with_generation_and_referrer_policy(
        source: &str,
        limits: &NativeEngineLimits,
        generation: u32,
        initial_referrer_policy: NativeFetchReferrerPolicy,
    ) -> Result<Self, NativeEngineError> {
        Self::parse_with_stylesheets_inline_policy_and_referrer_policy(
            source,
            limits,
            &[],
            generation,
            None,
            initial_referrer_policy,
        )
    }

    pub(crate) fn parse_with_stylesheets_inline_policy_and_referrer_policy(
        source: &str,
        limits: &NativeEngineLimits,
        external_stylesheets: &[String],
        generation: u32,
        allowed_inline_style_nodes: Option<&BTreeSet<u32>>,
        initial_referrer_policy: NativeFetchReferrerPolicy,
    ) -> Result<Self, NativeEngineError> {
        Self::parse_with_stylesheets_inline_policy_and_referrer_policy_with_ua_stylesheet(
            source,
            limits,
            external_stylesheets,
            generation,
            allowed_inline_style_nodes,
            initial_referrer_policy,
            true,
        )
    }

    fn parse_with_stylesheets_inline_policy_and_referrer_policy_with_ua_stylesheet(
        source: &str,
        limits: &NativeEngineLimits,
        external_stylesheets: &[String],
        generation: u32,
        allowed_inline_style_nodes: Option<&BTreeSet<u32>>,
        initial_referrer_policy: NativeFetchReferrerPolicy,
        include_user_agent_stylesheet: bool,
    ) -> Result<Self, NativeEngineError> {
        limits.validate()?;
        if source.len() > limits.max_document_bytes {
            return Err(NativeEngineError::limit(
                "HTML document",
                limits.max_document_bytes,
                source.len(),
            ));
        }
        let temporary_node_limit = limits.max_nodes.saturating_mul(2);
        let html_parse_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "html5_tree_construction",
        );
        let parsed = parse_html5(source, temporary_node_limit, limits.max_dom_depth);
        html_parse_stage.finish(Some(parsed.nodes.len()));
        if let Some(failure) = parsed.sink_failure {
            return Err(match failure {
                HtmlTreeSinkFailure::InvalidTree(reason) => NativeEngineError::Parse {
                    offset: 0,
                    reason: reason.into(),
                },
                HtmlTreeSinkFailure::NodeLimitExceeded { actual } => NativeEngineError::limit(
                    "temporary HTML parser nodes",
                    temporary_node_limit,
                    actual,
                ),
                HtmlTreeSinkFailure::DomDepthExceeded { actual } => {
                    NativeEngineError::limit("DOM depth", limits.max_dom_depth, actual)
                }
            });
        }
        let root_node = parsed
            .nodes
            .first()
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "HTML parser returned no document node".into(),
            })?;
        if !matches!(root_node.kind, HtmlParsedNodeKind::Document) {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "HTML parser returned an invalid document root".into(),
            });
        }

        let root = NativeNodeId {
            generation,
            index: 0,
        };
        let document_init_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "native_document_initialize",
        );
        let mut document = Self::empty();
        document_init_stage.finish(None);
        document.generation = generation;
        document.revision = u64::from(generation);
        document.document_referrer_policy = initial_referrer_policy;
        document.root = root;
        document.max_nodes = limits.max_nodes;
        document.max_dom_depth = limits.max_dom_depth;
        document.quirks_mode = parsed.quirks_mode;
        document.nodes[0].id = root;
        let materialize_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "html_dom_materialize",
        );
        document.append_html_parsed_nodes(
            &parsed.nodes,
            &root_node.children,
            root,
            0,
            limits.max_nodes,
            limits.max_dom_depth,
        )?;
        materialize_stage.finish(Some(document.nodes.len()));
        let finish_parse_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "html_document_finalize",
        );
        document.finish_html_parse(
            external_stylesheets,
            allowed_inline_style_nodes,
            parsed.parse_error_count,
            include_user_agent_stylesheet,
        )?;
        finish_parse_stage.finish(None);
        Ok(document)
    }

    pub(super) fn append_html_parsed_nodes(
        &mut self,
        parsed: &[HtmlParsedNode],
        node_indices: &[usize],
        parent: NativeNodeId,
        element_depth: usize,
        max_nodes: usize,
        max_dom_depth: usize,
    ) -> Result<(), NativeEngineError> {
        for node_index in node_indices {
            let node = parsed
                .get(*node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "HTML tree sink returned an invalid child index".into(),
                })?;
            match &node.kind {
                HtmlParsedNodeKind::Document | HtmlParsedNodeKind::DocumentFragment => {
                    self.append_html_parsed_nodes(
                        parsed,
                        &node.children,
                        parent,
                        element_depth,
                        max_nodes,
                        max_dom_depth,
                    )?;
                }
                HtmlParsedNodeKind::DocumentType {
                    name,
                    public_id,
                    system_id,
                } => {
                    self.add_node(
                        parent,
                        NativeNodeKind::DocumentType {
                            name: name.clone(),
                            public_id: (!public_id.is_empty()).then(|| public_id.clone()),
                            system_id: (!system_id.is_empty()).then(|| system_id.clone()),
                        },
                        max_nodes,
                    )?;
                }
                HtmlParsedNodeKind::Element {
                    name,
                    namespace,
                    attributes,
                } => {
                    if element_depth >= max_dom_depth {
                        return Err(NativeEngineError::limit(
                            "DOM depth",
                            max_dom_depth,
                            element_depth.saturating_add(1),
                        ));
                    }
                    let mut attribute_map = BTreeMap::new();
                    let mut attribute_namespaces = BTreeMap::new();
                    for attribute in attributes {
                        attribute_map
                            .entry(attribute.name.clone())
                            .or_insert_with(|| attribute.value.clone());
                        if let Some(attribute_namespace) = &attribute.namespace {
                            attribute_namespaces
                                .entry(attribute.name.clone())
                                .or_insert_with(|| attribute_namespace.clone());
                        }
                    }
                    let id = self.add_node(
                        parent,
                        NativeNodeKind::Element {
                            name: name.clone(),
                            attributes: attribute_map,
                        },
                        max_nodes,
                    )?;
                    if let Some(native_node) = self.raw_node_mut(id) {
                        native_node.state.namespace_uri = Some(namespace.clone());
                        native_node.state.attribute_namespaces = attribute_namespaces;
                    }
                    self.apply_meta_referrer_policy_to_node(id);
                    if let Some(template_contents_index) = node.template_contents {
                        let template_contents =
                            parsed.get(template_contents_index).ok_or_else(|| {
                                NativeEngineError::Parse {
                                    offset: 0,
                                    reason: "HTML template has invalid contents".into(),
                                }
                            })?;
                        if !matches!(
                            &template_contents.kind,
                            HtmlParsedNodeKind::DocumentFragment
                        ) {
                            return Err(NativeEngineError::Parse {
                                offset: 0,
                                reason: "HTML template contents are not a fragment".into(),
                            });
                        }
                        let fragment =
                            self.add_detached_node(NativeNodeKind::DocumentFragment, max_nodes)?;
                        if let Some(template) = self.raw_node_mut(id) {
                            template.template_content = Some(fragment);
                        }
                        if let Some(fragment_node) = self.raw_node_mut(fragment) {
                            fragment_node.template_host = Some(id);
                        }
                        self.append_html_parsed_nodes(
                            parsed,
                            &template_contents.children,
                            fragment,
                            element_depth.saturating_add(1),
                            max_nodes,
                            max_dom_depth,
                        )?;
                    }
                    self.append_html_parsed_nodes(
                        parsed,
                        &node.children,
                        id,
                        element_depth.saturating_add(1),
                        max_nodes,
                        max_dom_depth,
                    )?;
                }
                HtmlParsedNodeKind::Comment(value) => {
                    self.add_node(parent, NativeNodeKind::Comment(value.clone()), max_nodes)?;
                }
                HtmlParsedNodeKind::Text(value) => {
                    self.add_node(parent, NativeNodeKind::Text(value.clone()), max_nodes)?;
                }
                HtmlParsedNodeKind::ProcessingInstruction { .. } => {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "HTML tree sink returned a processing instruction".into(),
                    });
                }
            }
        }
        Ok(())
    }

    fn finish_html_parse(
        &mut self,
        external_stylesheets: &[String],
        allowed_inline_style_nodes: Option<&BTreeSet<u32>>,
        parse_error_count: usize,
        include_user_agent_stylesheet: bool,
    ) -> Result<(), NativeEngineError> {
        if let Some(allowed_inline_style_nodes) = allowed_inline_style_nodes {
            self.set_inline_style_policy(allowed_inline_style_nodes);
        }
        let mut style_sources = self
            .nodes
            .iter()
            .filter(|node| node.element_name() == Some("style") && node.inline_style_allowed())
            .map(|node| {
                let mut source = String::new();
                self.collect_raw_text(node.id(), &mut source);
                source
            })
            .collect::<Vec<_>>();
        style_sources.extend(external_stylesheets.iter().cloned());
        let style_sources = if include_user_agent_stylesheet {
            with_html_user_agent_stylesheet(style_sources)
        } else {
            style_sources
        };
        let mut diagnostics = NativeDiagnosticSink::default();
        if parse_error_count > 0 {
            diagnostics.push(
                NativeDiagnosticCode::MalformedHtml,
                NativeDiagnosticSource::Document,
                0,
                &format!("html-parse-errors-{parse_error_count}"),
            );
        }
        self.stylesheet =
            NativeStylesheet::from_sources_with_diagnostics(style_sources, &mut diagnostics)?;
        self.background_image_sources = self.stylesheet.background_image_sources().clone();
        for node in &self.nodes {
            if !node.inline_style_allowed() {
                continue;
            }
            let Some(inline_style) = node.attribute("style") else {
                continue;
            };
            collect_background_image_sources(inline_style, &mut self.background_image_sources);
            super::super::css::collect_declaration_diagnostics(
                inline_style,
                NativeDiagnosticSource::InlineStyle {
                    node_index: node.id().index(),
                },
                0,
                &mut diagnostics,
            );
        }
        let (diagnostics, diagnostics_truncated) = diagnostics.finish();
        self.diagnostics = diagnostics;
        self.diagnostics_truncated = diagnostics_truncated;
        self.normalize_select_defaults();
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn parse_legacy_with_stylesheets_and_inline_style_policy(
        source: &str,
        limits: &NativeEngineLimits,
        external_stylesheets: &[String],
        generation: u32,
        allowed_inline_style_nodes: Option<&BTreeSet<u32>>,
    ) -> Result<Self, NativeEngineError> {
        limits.validate()?;
        if source.len() > limits.max_document_bytes {
            return Err(NativeEngineError::limit(
                "HTML document",
                limits.max_document_bytes,
                source.len(),
            ));
        }
        let tokens = tokenize(source, limits.max_nodes.saturating_mul(2).saturating_add(1))?;
        let root = NativeNodeId {
            generation,
            index: 0,
        };
        let mut document = Self {
            generation,
            revision: u64::from(generation),
            root,
            max_nodes: limits.max_nodes,
            max_dom_depth: limits.max_dom_depth,
            quirks_mode: super::super::html_parser::HtmlParsedQuirksMode::NoQuirks,
            document_referrer_policy: NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            inherited_base_url: None,
            nodes: vec![NativeNode {
                id: root,
                parent: None,
                children: Vec::new(),
                template_content: None,
                template_host: None,
                shadow_host: None,
                shadow_root: None,
                shadow_mode: None,
                kind: NativeNodeKind::Document,
                state: NativeElementState::default(),
            }],
            stylesheet: NativeStylesheet::default(),
            font_resources: Vec::new(),
            font_book: NativeFontBook::system(),
            external_stylesheet_states: BTreeMap::new(),
            external_image_preload_states: BTreeMap::new(),
            preloaded_image_resources: BTreeMap::new(),
            viewport: Viewport::default(),
            css_target_fragment: None,
            computed_styles: None,
            layout_style_cache: NativeLayoutStyleCache::default(),
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
            script_node_ids: BTreeMap::new(),
            started_script_nodes: BTreeSet::new(),
            processed_csp_meta_nodes: BTreeSet::new(),
            pending_csp_meta_policies: BTreeMap::new(),
            embedded_frame_owner_removal_sequences: BTreeMap::new(),
            next_embedded_frame_owner_removal_sequence: 0,
            image_resources: BTreeMap::new(),
            image_loads: BTreeMap::new(),
            video_poster_resources: BTreeMap::new(),
            video_poster_loads: BTreeMap::new(),
            media_resources: BTreeMap::new(),
            media_loads: BTreeMap::new(),
            media_errors: BTreeMap::new(),
            background_image_sources: BTreeMap::new(),
            background_image_resources: BTreeMap::new(),
            canvas_resources: BTreeMap::new(),
            inline_style_element_reports: BTreeMap::new(),
            inline_style_attribute_reports: BTreeMap::new(),
        };
        let mut stack = vec![root];
        let mut active_formatting = ActiveFormattingList::new();
        let mut document_type_seen = false;

        for token in tokens {
            match token {
                HtmlToken::StartTag {
                    name,
                    attributes,
                    mut self_closing,
                } => {
                    if prepare_foreign_content_breakout_start_tag(
                        &document,
                        &mut stack,
                        &name,
                        &attributes,
                        &mut active_formatting,
                    )? {
                        // The self-closing flag is ignored when the token is
                        // reprocessed under HTML tree-construction rules.
                        self_closing = false;
                    }
                    if name == "table" {
                        let stack_before = stack.clone();
                        let process_table =
                            prepare_html_table_start_token(&document, &mut stack, None)?;
                        clear_markers_for_popped_elements(
                            &document,
                            &stack_before,
                            &stack,
                            &mut active_formatting,
                        );
                        if !process_table {
                            continue;
                        }
                    }
                    let stack_len_before_cell_recovery = stack.len();
                    prepare_html_table_cell_structural_start_token(
                        &document, &mut stack, None, &name,
                    )?;
                    if stack.len() < stack_len_before_cell_recovery {
                        clear_active_formatting_to_marker(&mut active_formatting);
                    }
                    while stack.len() > 1
                        && stack.last().is_some_and(|id| {
                            document
                                .node(*id)
                                .and_then(|node| match node.kind() {
                                    NativeNodeKind::Element { name, .. } => Some(name.as_str()),
                                    NativeNodeKind::Document
                                    | NativeNodeKind::DocumentFragment
                                    | NativeNodeKind::DocumentType { .. }
                                    | NativeNodeKind::Comment(_)
                                    | NativeNodeKind::Text(_) => None,
                                })
                                .is_some_and(|current| should_auto_close(current, &name))
                        })
                    {
                        stack.pop();
                    }
                    document.insert_implied_html_table_elements(&mut stack, &name)?;
                    if !is_table_special_start_tag(&name, &attributes) {
                        reconstruct_active_formatting_elements(
                            &mut document,
                            &mut stack,
                            &mut active_formatting,
                            None,
                            false,
                        )?;
                        let nested_formatting_start =
                            (name == "a" && active_formatting_entry_index(&document, &active_formatting, "a").is_some())
                                || (name == "nobr"
                                    && active_formatting_entry_index(
                                        &document,
                                        &active_formatting,
                                        "nobr",
                                    )
                                    .is_some_and(|index| {
                                        matches!(active_formatting[index], ActiveFormattingEntry::Element(id)
                                            if html_element_is_in_scope(&document, &stack, id, None))
                                    }));
                        if nested_formatting_start {
                            adopt_active_formatting_element(
                                &mut document,
                                &mut stack,
                                &mut active_formatting,
                                None,
                                &name,
                            )?;
                            reconstruct_active_formatting_elements(
                                &mut document,
                                &mut stack,
                                &mut active_formatting,
                                None,
                                false,
                            )?;
                        }
                    }
                    let foster_location = if is_table_special_start_tag(&name, &attributes) {
                        None
                    } else {
                        html_table_foster_location(&document, &stack, None)?
                    };
                    let parent = foster_location
                        .map(|(parent, _)| parent)
                        .or_else(|| stack.last().copied())
                        .ok_or_else(|| NativeEngineError::Parse {
                            offset: 0,
                            reason: "tree builder lost its document root".into(),
                        })?;
                    let current_depth =
                        stack.iter().position(|id| *id == parent).ok_or_else(|| {
                            NativeEngineError::Parse {
                                offset: 0,
                                reason: "tree builder insertion parent is not open".into(),
                            }
                        })?;
                    if current_depth >= limits.max_dom_depth {
                        return Err(NativeEngineError::limit(
                            "DOM depth",
                            limits.max_dom_depth,
                            current_depth.saturating_add(1),
                        ));
                    }
                    let kind = NativeNodeKind::Element {
                        name: name.clone(),
                        attributes,
                    };
                    let id = match foster_location {
                        Some((parent, Some(before))) => {
                            document.add_node_before(parent, before, kind, limits.max_nodes)?
                        }
                        Some((parent, None)) => {
                            document.add_node(parent, kind, limits.max_nodes)?
                        }
                        None => document.add_node(parent, kind, limits.max_nodes)?,
                    };
                    document.assign_parsed_namespace_to_node(id, parent)?;
                    document.adjust_parsed_foreign_attributes(id)?;
                    let html_namespace_element = document
                        .raw_node(id)
                        .is_some_and(|node| node.namespace_uri() == Some(HTML_NAMESPACE_URI));
                    let html_void_element = html_namespace_element && is_void_element(&name);
                    if (!self_closing || html_namespace_element) && !html_void_element {
                        stack.push(id);
                        if document
                            .raw_node(id)
                            .is_some_and(|node| node.namespace_uri() == Some(HTML_NAMESPACE_URI))
                        {
                            if is_active_formatting_marker_element(&name) {
                                active_formatting.push(ActiveFormattingEntry::Marker);
                            } else if is_html_formatting_element(&name) {
                                push_active_formatting_element(
                                    &document,
                                    &mut active_formatting,
                                    id,
                                )?;
                            }
                        }
                    }
                }
                HtmlToken::EndTag(name) => {
                    let foreign_end_tag = prepare_foreign_content_end_tag(
                        &document,
                        &mut stack,
                        &name,
                        &mut active_formatting,
                        false,
                    )?;
                    if foreign_end_tag == ForeignContentEndTagDisposition::Consumed {
                        continue;
                    }
                    let stack_before = is_marker_sensitive_end_tag(&name).then(|| stack.clone());
                    let consumed = (name == "table"
                        && consume_html_table_end_token(&document, &mut stack, None)?)
                        || consume_html_table_column_group_end_token(
                            &document, &mut stack, None, &name,
                        )?
                        || (matches!(
                            name.as_str(),
                            "tbody" | "tfoot" | "thead" | "tr" | "td" | "th"
                        ) && consume_html_table_structure_end_token(
                            &document, &mut stack, None, &name,
                        )?)
                        || consume_html_table_cell_ignored_end_token(
                            &document, &stack, None, &name,
                        )?
                        || consume_html_table_row_ignored_end_token(
                            &document, &stack, None, &name,
                        )?
                        || consume_html_table_caption_ignored_end_token(
                            &document, &stack, None, &name,
                        )?
                        || consume_html_table_mode_ignored_end_token(
                            &document, &stack, None, &name,
                        )?;
                    if consumed {
                        if let Some(before) = stack_before.as_deref() {
                            clear_markers_for_popped_elements(
                                &document,
                                before,
                                &stack,
                                &mut active_formatting,
                            );
                        }
                        continue;
                    }
                    if name == "br" {
                        reconstruct_active_formatting_elements(
                            &mut document,
                            &mut stack,
                            &mut active_formatting,
                            None,
                            false,
                        )?;
                        insert_html_end_tag_recovery_element(
                            &mut document,
                            &mut stack,
                            None,
                            false,
                            "br",
                            false,
                        )?;
                        continue;
                    }
                    if name == "p" && !html_paragraph_is_in_button_scope(&document, &stack, None) {
                        reconstruct_active_formatting_elements(
                            &mut document,
                            &mut stack,
                            &mut active_formatting,
                            None,
                            false,
                        )?;
                        insert_html_end_tag_recovery_element(
                            &mut document,
                            &mut stack,
                            None,
                            false,
                            "p",
                            true,
                        )?;
                    }
                    if is_html_formatting_element(&name) {
                        if adopt_active_formatting_element(
                            &mut document,
                            &mut stack,
                            &mut active_formatting,
                            None,
                            &name,
                        )? {
                            continue;
                        }
                        if consume_formatting_fallback_end_tag(&document, &mut stack, None, &name) {
                            continue;
                        }
                    }
                    process_html_any_other_end_tag(&document, &mut stack, &name)?;
                    if let Some(before) = stack_before.as_deref() {
                        clear_markers_for_popped_elements(
                            &document,
                            before,
                            &stack,
                            &mut active_formatting,
                        );
                    }
                }
                HtmlToken::Text(value) => {
                    if !value.is_empty() {
                        let current = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                            offset: 0,
                            reason: "tree builder lost its text insertion mode".into(),
                        })?;
                        let foreign_content =
                            parser_uses_foreign_character_rules(&document, current);
                        let decoded = decode_html_character_data(&value, foreign_content);
                        if decoded.is_empty() {
                            continue;
                        }
                        reconstruct_active_formatting_elements(
                            &mut document,
                            &mut stack,
                            &mut active_formatting,
                            None,
                            false,
                        )?;
                        let foster_location = if decoded
                            .as_bytes()
                            .iter()
                            .any(|byte| !byte.is_ascii_whitespace())
                        {
                            html_table_foster_location(&document, &stack, None)?
                        } else {
                            None
                        };
                        let parent = foster_location
                            .map(|(parent, _)| parent)
                            .or_else(|| stack.last().copied())
                            .ok_or_else(|| NativeEngineError::Parse {
                                offset: 0,
                                reason: "tree builder lost its document root".into(),
                            })?;
                        let kind = NativeNodeKind::Text(decoded);
                        match foster_location {
                            Some((parent, Some(before))) => {
                                document.add_node_before(parent, before, kind, limits.max_nodes)?;
                            }
                            Some((parent, None)) => {
                                document.add_node(parent, kind, limits.max_nodes)?;
                            }
                            None => {
                                document.add_node(parent, kind, limits.max_nodes)?;
                            }
                        }
                    }
                }
                HtmlToken::RawText(value) => {
                    if !value.is_empty() {
                        let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                            offset: 0,
                            reason: "tree builder lost its document root".into(),
                        })?;
                        if document
                            .raw_node(parent)
                            .is_some_and(|node| node.namespace_uri() != Some(HTML_NAMESPACE_URI))
                        {
                            document.apply_script_inner_html(
                                parent.index,
                                &value,
                                &BTreeMap::new(),
                            )?;
                        } else {
                            document.add_node(
                                parent,
                                NativeNodeKind::Text(value),
                                limits.max_nodes,
                            )?;
                        }
                    }
                }
                HtmlToken::RcData(value) => {
                    let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                        offset: 0,
                        reason: "tree builder lost its RCDATA parent".into(),
                    })?;
                    if document
                        .raw_node(parent)
                        .is_some_and(|node| node.namespace_uri() != Some(HTML_NAMESPACE_URI))
                    {
                        document.apply_script_inner_html(parent.index, &value, &BTreeMap::new())?;
                    } else {
                        let decoded = decode_html_character_data(&value, false);
                        if !decoded.is_empty() {
                            document.add_node(
                                parent,
                                NativeNodeKind::Text(decoded),
                                limits.max_nodes,
                            )?;
                        }
                    }
                }
                HtmlToken::Comment(value) => {
                    let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                        offset: 0,
                        reason: "tree builder lost its document root".into(),
                    })?;
                    document.add_node(parent, NativeNodeKind::Comment(value), limits.max_nodes)?;
                }
                HtmlToken::Doctype {
                    name,
                    public_id,
                    system_id,
                } => {
                    // A document has at most one doctype, and HTML ignores a
                    // doctype token once the document element has started.
                    // Keeping that rule in the Rust owner prevents malformed
                    // input from creating ambiguous document.doctype state.
                    let document_element_started = document.node(root).is_some_and(|node| {
                        node.children().iter().any(|child| {
                            document.node(*child).is_some_and(|candidate| {
                                matches!(candidate.kind(), NativeNodeKind::Element { .. })
                            })
                        })
                    });
                    if !document_type_seen && !document_element_started {
                        document.add_node(
                            root,
                            NativeNodeKind::DocumentType {
                                name,
                                public_id,
                                system_id,
                            },
                            limits.max_nodes,
                        )?;
                        document_type_seen = true;
                    }
                }
            }
        }
        document.assign_parsed_namespaces();
        if let Some(allowed_inline_style_nodes) = allowed_inline_style_nodes {
            document.set_inline_style_policy(allowed_inline_style_nodes);
        }
        let mut style_sources = document
            .nodes
            .iter()
            .filter(|node| node.element_name() == Some("style") && node.inline_style_allowed())
            .map(|node| {
                let mut source = String::new();
                document.collect_raw_text(node.id(), &mut source);
                source
            })
            .collect::<Vec<_>>();
        style_sources.extend(external_stylesheets.iter().cloned());
        let style_sources = with_html_user_agent_stylesheet(style_sources);
        let mut diagnostics = NativeDiagnosticSink::default();
        document.stylesheet =
            NativeStylesheet::from_sources_with_diagnostics(style_sources, &mut diagnostics)?;
        document.background_image_sources = document.stylesheet.background_image_sources().clone();
        for node in &document.nodes {
            if !node.inline_style_allowed() {
                continue;
            }
            let Some(inline_style) = node.attribute("style") else {
                continue;
            };
            collect_background_image_sources(inline_style, &mut document.background_image_sources);
            super::super::css::collect_declaration_diagnostics(
                inline_style,
                NativeDiagnosticSource::InlineStyle {
                    node_index: node.id().index(),
                },
                0,
                &mut diagnostics,
            );
        }
        let (diagnostics, diagnostics_truncated) = diagnostics.finish();
        document.diagnostics = diagnostics;
        document.diagnostics_truncated = diagnostics_truncated;
        document.normalize_select_defaults();
        Ok(document)
    }
}

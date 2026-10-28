use super::*;

impl NativeDocument {
    pub(in crate::native_engine) fn apply_script_canvas(
        &mut self,
        node_index: u32,
        width: u32,
        height: u32,
        pixels_base64: &str,
        origin_clean: bool,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if width == 0
            || height == 0
            || width > MAX_NATIVE_CANVAS_DIMENSION
            || height > MAX_NATIVE_CANVAS_DIMENSION
        {
            return Err(NativeEngineError::invalid(
                "script canvas dimensions",
                format!("must be between 1 and {MAX_NATIVE_CANVAS_DIMENSION} pixels per axis"),
            ));
        }
        let expected_bytes = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                NativeEngineError::limit(
                    "script canvas pixels",
                    MAX_NATIVE_CANVAS_BYTES,
                    usize::MAX,
                )
            })?;
        if expected_bytes > MAX_NATIVE_CANVAS_BYTES {
            return Err(NativeEngineError::limit(
                "script canvas pixels",
                MAX_NATIVE_CANVAS_BYTES,
                expected_bytes,
            ));
        }
        let encoded_limit =
            (MAX_NATIVE_CANVAS_BYTES.saturating_add(2).saturating_div(3)).saturating_mul(4);
        if pixels_base64.len() > encoded_limit {
            return Err(NativeEngineError::limit(
                "script canvas pixels",
                encoded_limit,
                pixels_base64.len(),
            ));
        }
        let pixels = base64::engine::general_purpose::STANDARD
            .decode(pixels_base64)
            .map_err(|_| NativeEngineError::invalid("script canvas pixels", "invalid base64"))?;
        if pixels.len() != expected_bytes {
            return Err(NativeEngineError::invalid(
                "script canvas pixels",
                "pixel data does not match the canvas dimensions",
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("canvas") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "canvas commit requires a canvas element".into(),
            });
        }
        self.canvas_resources.insert(
            id.index(),
            NativeCanvasResource {
                width,
                height,
                pixels,
                origin_clean,
            },
        );
        Ok(())
    }

    pub(super) fn apply_script_custom_validity(
        &mut self,
        node_index: u32,
        message: &str,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if message.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "custom validity message",
                MAX_LOCATOR_BYTES,
                message.len(),
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node_mut(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if !matches!(
            node.element_name(),
            Some("button" | "input" | "select" | "textarea")
        ) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "custom validity requires a form-associated control".into(),
            });
        }
        node.state.custom_validity = message.to_owned();
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(super) fn apply_script_form_associated_disabled_state(
        &mut self,
        node_index: u32,
        disabled: bool,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node_mut(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.namespace_uri() != Some(HTML_NAMESPACE_URI)
            || !node.element_name().is_some_and(|name| name.contains('-'))
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason:
                    "form-associated disabled state requires a hyphenated HTML custom-element name"
                        .into(),
            });
        }
        node.state.form_associated_disabled = Some(disabled);
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(super) fn validation_events(
        &self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() == Some("form") {
            return Ok(self
                .invalid_form_controls_for_api(id)?
                .into_iter()
                .map(|id| (id, NativeEventKind::Invalid))
                .collect());
        }
        let (validity, _, will_validate) = self.script_validation_snapshot(id);
        if !will_validate || validity.valid {
            return Ok(Vec::new());
        }
        Ok(vec![(id, NativeEventKind::Invalid)])
    }

    pub(crate) fn apply_script_focus(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let Some(node) = self.node(id) else {
            return Err(NativeEngineError::DetachedTarget);
        };
        if self.is_inert(id) {
            return Ok(Vec::new());
        }
        if node.element_name() != Some("area")
            && node
                .attribute("tabindex")
                .and_then(parse_native_tab_index)
                .is_some()
        {
            if self.is_hidden(id) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "hidden targets cannot receive focus".into(),
                });
            }
            if self.is_actually_disabled_native_focus_target(id) {
                return Err(NativeEngineError::DisabledTarget);
            }
            if !self.can_focus_explicit_tabindex(id) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "target is not a rendered focusable element".into(),
                });
            }
            return Ok(self.focus_element(id));
        }
        let Some(semantic) = self.semantic_node(id) else {
            if self.node(id).and_then(NativeNode::element_name) == Some("area")
                && self.image_map_for_area(id).is_some()
            {
                return Ok(Vec::new());
            }
            return Err(NativeEngineError::TargetNotActionable {
                reason: "target has no supported semantic control role".into(),
            });
        };
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets cannot receive focus".into(),
            });
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        if !matches!(
            semantic.role.as_str(),
            "button" | "link" | "checkbox" | "radio" | "textbox" | "combobox" | "option"
        ) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "only supported semantic controls can receive focus".into(),
            });
        }
        if self.node(id).and_then(NativeNode::element_name) == Some("area") {
            let Some(anchor) = self
                .image_map_area_focus_anchors(id)
                .into_iter()
                .find(|anchor| !self.is_inert(*anchor))
            else {
                return Ok(Vec::new());
            };
            return Ok(self.focus_element_with_anchor(id, Some(anchor)));
        }
        Ok(self.focus_element(id))
    }

    pub(crate) fn apply_script_blur(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        if !node.state.focused {
            return Ok(Vec::new());
        }
        let state = &mut self
            .node_mut(id)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state;
        state.focused = false;
        state.focus_anchor_node_index = None;
        self.invalidate_layout_styles();
        Ok(vec![
            (id, NativeEventKind::Blur),
            (id, NativeEventKind::FocusOut),
        ])
    }

    pub(super) fn apply_script_value(
        &mut self,
        node_index: u32,
        value: &str,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "script value",
                MAX_LOCATOR_BYTES,
                value.len(),
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let element_name = self
            .script_node(id, script_nodes)
            .and_then(NativeNode::element_name)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if element_name == "select" {
            let matching = self.select_option_ids(id).into_iter().find(|option_id| {
                self.option_value(*option_id)
                    .is_some_and(|option_value| option_value == value)
            });
            for option_id in self.select_option_ids(id) {
                self.node_mut(option_id)
                    .ok_or(NativeEngineError::DetachedTarget)?
                    .state
                    .selected = matching == Some(option_id);
            }
            self.invalidate_layout_styles();
            return Ok(());
        }
        if element_name == "input"
            && self
                .node(id)
                .and_then(|node| node.attribute("type"))
                .is_some_and(|input_type| input_type.eq_ignore_ascii_case("file"))
        {
            if !value.is_empty() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "script cannot assign a non-empty file input value".into(),
                });
            }
            let node = self
                .script_node_mut(id, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?;
            node.state.files.clear();
            node.state.value = Some(String::new());
            self.invalidate_layout_styles();
            return Ok(());
        }
        if !matches!(element_name, "input" | "textarea") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script value requires an input or textarea".into(),
            });
        }
        {
            let node = self
                .script_node_mut(id, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?;
            node.state.value = Some(value.to_owned());
        }
        if self
            .semantic_node(id)
            .is_some_and(|semantic| semantic.role == "textbox")
        {
            let end = value.chars().count();
            self.set_selection_state(id, end, end, "none")?;
        }
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(super) fn apply_script_checked(
        &mut self,
        node_index: u32,
        checked: bool,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("input")
            || !matches!(
                node.attribute("type").unwrap_or("text"),
                "checkbox" | "radio"
            )
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script checked state requires a checkbox or radio input".into(),
            });
        }
        if checked && node.attribute("type") == Some("radio") {
            let group_name = node.attribute("name").map(str::to_owned);
            let radio_ids = self
                .nodes
                .iter()
                .filter(|candidate| {
                    candidate.element_name() == Some("input")
                        && candidate.attribute("type") == Some("radio")
                })
                .filter(|candidate| {
                    candidate.id() == id
                        || group_name.as_deref().is_some_and(|name| {
                            !name.is_empty() && candidate.attribute("name") == Some(name)
                        })
                })
                .map(NativeNode::id)
                .collect::<Vec<_>>();
            for radio_id in radio_ids {
                self.node_mut(radio_id)
                    .ok_or(NativeEngineError::DetachedTarget)?
                    .state
                    .checked = radio_id == id;
            }
        } else {
            self.script_node_mut(id, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .checked = checked;
        }
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(super) fn apply_script_indeterminate(
        &mut self,
        node_index: u32,
        indeterminate: bool,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("input")
            || !node
                .attribute("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("checkbox"))
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script indeterminate state requires a checkbox input".into(),
            });
        }
        self.script_node_mut(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state
            .indeterminate = indeterminate;
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(super) fn apply_script_selected(
        &mut self,
        id: NativeNodeId,
        selected: bool,
    ) -> Result<(), NativeEngineError> {
        if self.node(id).and_then(NativeNode::element_name) != Some("option") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script selected state requires an option".into(),
            });
        }
        let select_id = self.select_for_option(id)?;
        let multiple = self
            .node(select_id)
            .is_some_and(|node| node.attribute("multiple").is_some());
        if selected && multiple {
            self.node_mut(id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .selected = true;
        } else if selected {
            for option_id in self.select_option_ids(select_id) {
                self.node_mut(option_id)
                    .ok_or(NativeEngineError::DetachedTarget)?
                    .state
                    .selected = option_id == id;
            }
        } else {
            self.node_mut(id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .selected = false;
        }
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(super) fn apply_script_attribute(
        &mut self,
        node_index: u32,
        name: &str,
        value: &str,
        namespace_uri: Option<&str>,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let name = validate_script_attribute(name)?;
        let namespace_uri = match namespace_uri {
            None | Some("") => None,
            Some(value) => Some(validate_attribute_namespace_uri(value)?),
        };
        if value.len() > MAX_ATTRIBUTE_BYTES {
            return Err(NativeEngineError::limit(
                "script attribute value",
                MAX_ATTRIBUTE_BYTES,
                value.len(),
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let attribute_changed = self.script_node(id, script_nodes).is_some_and(|node| {
            let namespace_changed = node
                .state
                .attribute_namespaces
                .get(&name)
                .map(String::as_str)
                != namespace_uri.as_deref();
            let value_changed = match &node.kind {
                NativeNodeKind::Element { attributes, .. } => {
                    attributes.get(&name).map(String::as_str) != Some(value)
                }
                _ => true,
            };
            namespace_changed || value_changed
        });
        let node = self
            .script_node_mut(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let NativeNodeKind::Element { attributes, .. } = &mut node.kind else {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script attributes require an element".into(),
            });
        };
        attributes.insert(name.clone(), value.to_owned());
        if let Some(namespace_uri) = namespace_uri {
            node.state
                .attribute_namespaces
                .insert(name.clone(), namespace_uri);
        } else {
            node.state.attribute_namespaces.remove(&name);
        }
        if crate::native_engine::content_process::runtime_diagnostic_trace_enabled() {
            let category = match name.as_str() {
                "class" => "class",
                "style" => "style",
                "id" => "id",
                "width" | "height" | "hidden" => "dimension_or_visibility",
                "src" | "srcset" => "image_or_resource_source",
                "href" => "link_target",
                name if name.starts_with("aria-") => "aria",
                name if name.starts_with("data-") => "data",
                _ => "other",
            };
            crate::native_engine::content_process::ContentLoadStage::begin_with_resource(
                0,
                "script_attribute_update",
                None,
                Some(category),
            )
            .finish(None);
        }
        if attribute_changed {
            self.invalidate_layout_styles();
        }
        self.apply_meta_referrer_policy_to_node(id);
        Ok(())
    }

    pub(super) fn remove_script_attribute(
        &mut self,
        node_index: u32,
        name: &str,
        namespace_uri: Option<&str>,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let name = validate_script_attribute(name)?;
        let namespace_uri = match namespace_uri {
            None | Some("") => None,
            Some(value) => Some(validate_attribute_namespace_uri(value)?),
        };
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node_mut(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let NativeNodeKind::Element { attributes, .. } = &mut node.kind else {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script attributes require an element".into(),
            });
        };
        if namespace_uri.as_deref()
            != node
                .state
                .attribute_namespaces
                .get(&name)
                .map(String::as_str)
            && namespace_uri.is_some()
        {
            return Ok(());
        }
        attributes.remove(&name);
        node.state.attribute_namespaces.remove(&name);
        self.invalidate_layout_styles();
        self.apply_meta_referrer_policy_to_node(id);
        Ok(())
    }

    pub(super) fn apply_script_text_content(
        &mut self,
        node_index: u32,
        value: &str,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "script text content",
                MAX_LOCATOR_BYTES,
                value.len(),
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let kind = self
            .script_node(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?
            .kind()
            .clone();
        if matches!(kind, NativeNodeKind::Text(_) | NativeNodeKind::Comment(_)) {
            let node = self
                .script_node_mut(id, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?;
            node.kind = match node.kind() {
                NativeNodeKind::Comment(_) => NativeNodeKind::Comment(value.to_owned()),
                _ => NativeNodeKind::Text(value.to_owned()),
            };
            self.invalidate_layout_styles();
            return Ok(());
        }
        if !matches!(kind, NativeNodeKind::Element { .. }) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script text content requires an element".into(),
            });
        }
        let children = self
            .script_node(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?
            .children
            .to_vec();
        for child in children {
            self.detach_subtree(child)?;
        }
        if !value.is_empty() {
            self.add_node(id, NativeNodeKind::Text(value.to_owned()), self.max_nodes)?;
        }
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(super) fn apply_script_document_title(
        &mut self,
        value: &str,
    ) -> Result<(), NativeEngineError> {
        if value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "script document title",
                MAX_LOCATOR_BYTES,
                value.len(),
            ));
        }
        let title_id = if let Some(title_id) = self.find_element(self.root, "title") {
            title_id
        } else {
            let parent = if let Some(head_id) = self.find_element(self.root, "head") {
                head_id
            } else if let Some(html_id) = self.find_element(self.root, "html") {
                if self.element_depth(html_id) >= self.max_dom_depth {
                    return Err(NativeEngineError::limit(
                        "DOM depth",
                        self.max_dom_depth,
                        self.element_depth(html_id).saturating_add(1),
                    ));
                }
                self.add_node(
                    html_id,
                    NativeNodeKind::Element {
                        name: "head".into(),
                        attributes: BTreeMap::new(),
                    },
                    self.max_nodes,
                )?
            } else {
                self.root
            };
            if parent != self.root && self.element_depth(parent) >= self.max_dom_depth {
                return Err(NativeEngineError::limit(
                    "DOM depth",
                    self.max_dom_depth,
                    self.element_depth(parent).saturating_add(1),
                ));
            }
            self.add_node(
                parent,
                NativeNodeKind::Element {
                    name: "title".into(),
                    attributes: BTreeMap::new(),
                },
                self.max_nodes,
            )?
        };
        self.apply_script_text_content(title_id.index(), value, &BTreeMap::new())
    }

    #[cfg(test)]
    pub(super) fn insert_implied_html_table_elements(
        &mut self,
        stack: &mut Vec<NativeNodeId>,
        incoming_name: &str,
    ) -> Result<(), NativeEngineError> {
        let Some(current) = stack.last().and_then(|id| self.raw_node(*id)) else {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder lost its current node".into(),
            });
        };
        if current.namespace_uri() != Some(HTML_NAMESPACE_URI) {
            return Ok(());
        }
        let implied_names: &[&str] = match (current.element_name(), incoming_name) {
            (Some("table"), "col") => &["colgroup"],
            (Some("table"), "tr") => &["tbody"],
            (Some("table"), "td" | "th") => &["tbody", "tr"],
            (Some("tbody" | "thead" | "tfoot"), "td" | "th") => &["tr"],
            _ => &[],
        };
        for name in implied_names {
            let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder lost its insertion parent".into(),
            })?;
            let parent_depth = self.element_depth(parent);
            if parent_depth >= self.max_dom_depth {
                return Err(NativeEngineError::limit(
                    "DOM depth",
                    self.max_dom_depth,
                    parent_depth.saturating_add(1),
                ));
            }
            let implied = self.add_node(
                parent,
                NativeNodeKind::Element {
                    name: (*name).to_owned(),
                    attributes: BTreeMap::new(),
                },
                self.max_nodes,
            )?;
            self.assign_parsed_namespace_to_node(implied, parent)?;
            stack.push(implied);
        }
        Ok(())
    }

    pub(super) fn apply_script_inner_html(
        &mut self,
        node_index: u32,
        value: &str,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "script innerHTML",
                MAX_LOCATOR_BYTES,
                value.len(),
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let (
            context_name,
            context_namespace,
            context_attributes,
            old_children,
            template_fragment,
            context_depth,
        ) = {
            let target = self
                .script_node(id, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?;
            let Some(context_name) = target.element_name() else {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "script innerHTML requires an element".into(),
                });
            };
            let context_attributes = target
                .attributes()
                .into_iter()
                .flat_map(|attributes| attributes.iter())
                .map(|(name, value)| HtmlParsedAttribute {
                    name: name.clone(),
                    value: value.clone(),
                    namespace: target.state.attribute_namespaces.get(name).cloned(),
                })
                .collect::<Vec<_>>();
            let template_fragment = (context_name == "template"
                && target.namespace_uri() == Some(HTML_NAMESPACE_URI))
            .then_some(target.template_content)
            .flatten();
            let old_children = template_fragment
                .and_then(|fragment| self.raw_node(fragment))
                .map_or_else(
                    || target.children().to_vec(),
                    |fragment| fragment.children().to_vec(),
                );
            (
                context_name.to_owned(),
                target
                    .namespace_uri()
                    .unwrap_or(HTML_NAMESPACE_URI)
                    .to_owned(),
                context_attributes,
                old_children,
                template_fragment,
                self.element_depth(id),
            )
        };
        if context_name.is_empty() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script innerHTML requires an element".into(),
            });
        }

        let temporary_node_limit = self.max_nodes.saturating_mul(2);
        let parsed = parse_html5_fragment(
            value,
            HtmlFragmentOptions {
                context_namespace: &context_namespace,
                context_name: &context_name,
                context_attributes: &context_attributes,
                scripting_enabled: true,
                context_element_depth: context_depth,
                max_nodes: temporary_node_limit,
                max_dom_depth: self.max_dom_depth,
            },
        )
        .map_err(|failure| match failure {
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
                NativeEngineError::limit("DOM depth", self.max_dom_depth, actual)
            }
        })?;
        let fragment_root = parsed
            .nodes
            .first()
            .filter(|root| matches!(&root.kind, HtmlParsedNodeKind::DocumentFragment))
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "HTML fragment parser returned an invalid root".into(),
            })?;
        let parsed_node_count = self.count_validated_html_fragment_nodes(
            &parsed,
            &fragment_root.children,
            context_depth,
        )?;
        let needs_template_fragment = context_name == "template"
            && context_namespace == HTML_NAMESPACE_URI
            && template_fragment.is_none();
        let required_nodes = parsed_node_count.saturating_add(usize::from(needs_template_fragment));
        let available_nodes = self.max_nodes.saturating_sub(self.nodes.len());
        if required_nodes > available_nodes {
            return Err(NativeEngineError::limit(
                "DOM nodes",
                self.max_nodes,
                self.nodes.len().saturating_add(required_nodes),
            ));
        }

        // Parse and validate the complete replacement before detaching the old
        // subtree. A typed parser or budget failure therefore leaves it intact.
        for child in old_children {
            self.detach_subtree(child)?;
        }
        let replacement_parent = if let Some(fragment) = template_fragment {
            fragment
        } else if needs_template_fragment {
            let fragment =
                self.add_detached_node(NativeNodeKind::DocumentFragment, self.max_nodes)?;
            if let Some(template) = self.raw_node_mut(id) {
                template.template_content = Some(fragment);
            }
            if let Some(fragment_node) = self.raw_node_mut(fragment) {
                fragment_node.template_host = Some(id);
            }
            fragment
        } else {
            id
        };
        self.append_html_parsed_nodes(
            &parsed.nodes,
            &fragment_root.children,
            replacement_parent,
            context_depth,
            self.max_nodes,
            self.max_dom_depth,
        )?;
        if parsed.parse_error_count > 0 {
            let detail = format!("html-fragment-parse-errors-{}", parsed.parse_error_count);
            if self.diagnostics.len() < super::super::diagnostics::MAX_NATIVE_DIAGNOSTICS {
                self.diagnostics.push(NativeDiagnostic::new(
                    NativeDiagnosticCode::MalformedHtml,
                    NativeDiagnosticSource::Document,
                    0,
                    &detail,
                ));
            } else {
                self.diagnostics_truncated = true;
            }
        }
        self.invalidate_layout_styles();
        self.capture_attached_content_security_policy_meta();
        Ok(())
    }

    pub(super) fn bind_inner_html_node_indexes(
        &self,
        root_id: NativeNodeId,
        temporary_node_indexes: &[u32],
        script_nodes: &mut BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if temporary_node_indexes.len() > self.max_nodes {
            return Err(NativeEngineError::limit(
                "innerHTML node identities",
                self.max_nodes,
                temporary_node_indexes.len(),
            ));
        }
        let mut seen_temporary_indexes = BTreeSet::new();
        for temporary_index in temporary_node_indexes {
            if *temporary_index < SCRIPT_TEMP_NODE_BASE
                || script_nodes.contains_key(temporary_index)
                || !seen_temporary_indexes.insert(*temporary_index)
            {
                return Err(NativeEngineError::invalid(
                    "innerHTML node identities",
                    "temporary node indexes must be unique, unmapped script-node identities",
                ));
            }
        }

        let root = self
            .raw_node(root_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let root_is_html_template = matches!(root.kind(), NativeNodeKind::Element { name, .. }
            if name.eq_ignore_ascii_case("template")
                && root.state.namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI));
        let root_children = if root_is_html_template {
            let fragment_id = root
                .template_content
                .ok_or(NativeEngineError::DetachedTarget)?;
            self.raw_node(fragment_id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .children()
                .to_vec()
        } else {
            root.children().to_vec()
        };
        let mut pending = root_children
            .into_iter()
            .rev()
            .map(|id| (id, 1usize))
            .collect::<Vec<_>>();
        let mut visited = 0usize;
        let mut node_ids = Vec::new();
        while let Some((id, depth)) = pending.pop() {
            visited = visited.saturating_add(1);
            if visited > self.max_nodes {
                return Err(NativeEngineError::limit(
                    "innerHTML identity tree nodes",
                    self.max_nodes,
                    visited,
                ));
            }
            if depth > self.max_dom_depth {
                return Err(NativeEngineError::limit(
                    "innerHTML identity tree depth",
                    self.max_dom_depth,
                    depth,
                ));
            }
            let node = self.raw_node(id).ok_or(NativeEngineError::DetachedTarget)?;
            match node.kind() {
                NativeNodeKind::Element { name, .. } => {
                    node_ids.push(id);
                    let is_html_template = name.eq_ignore_ascii_case("template")
                        && node.state.namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI);
                    if is_html_template {
                        if let Some(fragment_id) = node.template_content {
                            let fragment = self
                                .raw_node(fragment_id)
                                .ok_or(NativeEngineError::DetachedTarget)?;
                            for child in fragment.children().iter().rev().copied() {
                                pending.push((child, depth.saturating_add(1)));
                            }
                        }
                        continue;
                    }
                    for child in node.children().iter().rev().copied() {
                        pending.push((child, depth.saturating_add(1)));
                    }
                }
                NativeNodeKind::Text(_) | NativeNodeKind::Comment(_) => node_ids.push(id),
                NativeNodeKind::DocumentFragment => {
                    for child in node.children().iter().rev().copied() {
                        pending.push((child, depth.saturating_add(1)));
                    }
                }
                NativeNodeKind::Document | NativeNodeKind::DocumentType { .. } => {}
            }
        }
        if node_ids.len() != temporary_node_indexes.len() {
            return Err(NativeEngineError::invalid(
                "innerHTML node identities",
                "JavaScript and native fragment trees have different node counts",
            ));
        }
        for (temporary_index, node_id) in temporary_node_indexes.iter().zip(node_ids) {
            script_nodes.insert(*temporary_index, node_id);
        }
        Ok(())
    }

    pub(super) fn count_validated_html_fragment_nodes(
        &self,
        parsed: &HtmlParsedFragment,
        node_indices: &[usize],
        element_depth: usize,
    ) -> Result<usize, NativeEngineError> {
        let mut count = 0_usize;
        for node_index in node_indices {
            let node = parsed
                .nodes
                .get(*node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "HTML fragment parser returned an invalid child index".into(),
                })?;
            match &node.kind {
                HtmlParsedNodeKind::DocumentFragment => {
                    count = count.saturating_add(self.count_validated_html_fragment_nodes(
                        parsed,
                        &node.children,
                        element_depth,
                    )?);
                }
                HtmlParsedNodeKind::Element { .. } => {
                    if element_depth >= self.max_dom_depth {
                        return Err(NativeEngineError::limit(
                            "DOM depth",
                            self.max_dom_depth,
                            element_depth.saturating_add(1),
                        ));
                    }
                    count = count.saturating_add(1).saturating_add(
                        self.count_validated_html_fragment_nodes(
                            parsed,
                            &node.children,
                            element_depth.saturating_add(1),
                        )?,
                    );
                    if let Some(contents_index) = node.template_contents {
                        let contents = parsed.nodes.get(contents_index).ok_or_else(|| {
                            NativeEngineError::Parse {
                                offset: 0,
                                reason: "HTML fragment template has invalid contents".into(),
                            }
                        })?;
                        if !matches!(&contents.kind, HtmlParsedNodeKind::DocumentFragment) {
                            return Err(NativeEngineError::Parse {
                                offset: 0,
                                reason: "HTML fragment template contents are not a fragment".into(),
                            });
                        }
                        count = count.saturating_add(1).saturating_add(
                            self.count_validated_html_fragment_nodes(
                                parsed,
                                &contents.children,
                                element_depth.saturating_add(1),
                            )?,
                        );
                    }
                }
                HtmlParsedNodeKind::DocumentType { .. }
                | HtmlParsedNodeKind::Comment(_)
                | HtmlParsedNodeKind::Text(_) => {
                    count = count.saturating_add(1);
                }
                HtmlParsedNodeKind::Document | HtmlParsedNodeKind::ProcessingInstruction { .. } => {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "HTML fragment parser returned an unsupported node".into(),
                    });
                }
            }
        }
        Ok(count)
    }

    pub(super) fn apply_script_remove_node(
        &mut self,
        id: NativeNodeId,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if id == self.root {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "the document root cannot be removed".into(),
            });
        }
        let is_script_node = script_nodes.values().any(|candidate| *candidate == id);
        if (!is_script_node && !self.is_attached(id) && !self.belongs_to_template_content(id))
            || self.raw_node(id).is_none()
        {
            return Err(NativeEngineError::DetachedTarget);
        }
        self.detach_subtree(id)
    }

    pub(super) fn element_depth(&self, id: NativeNodeId) -> usize {
        let mut depth: usize = 0;
        let mut current = Some(id);
        while let Some(current_id) = current {
            let Some(node) = self.raw_node(current_id) else {
                break;
            };
            if node.element_name().is_some() {
                depth = depth.saturating_add(1);
            }
            current = node.parent().or(node.template_host);
        }
        depth
    }

    #[cfg(test)]
    pub(super) fn element_inner_html(&self, id: NativeNodeId, max_bytes: usize) -> String {
        let mut output = String::new();
        let mut truncated = false;
        let Some(element) = self.node(id) else {
            return output;
        };
        let raw_text_context = element.namespace_uri() == Some(HTML_NAMESPACE_URI)
            && element
                .element_name()
                .is_some_and(is_html_literal_text_element);
        let children = element
            .template_content
            .and_then(|fragment| self.raw_node(fragment))
            .map_or_else(
                || element.children().to_vec(),
                |fragment| fragment.children().to_vec(),
            );
        for child in children {
            self.append_serialized_node(
                child,
                max_bytes,
                &mut output,
                &mut truncated,
                raw_text_context,
            );
            if truncated {
                break;
            }
        }
        output
    }

    #[cfg(test)]
    pub(super) fn append_serialized_node(
        &self,
        id: NativeNodeId,
        max_bytes: usize,
        output: &mut String,
        truncated: &mut bool,
        raw_text: bool,
    ) {
        if *truncated {
            return;
        }
        let Some(node) = self.node(id) else {
            return;
        };
        match node.kind() {
            NativeNodeKind::Comment(value) => {
                append_bounded_markup(output, "<!--", max_bytes, truncated);
                append_bounded_markup(output, value, max_bytes, truncated);
                append_bounded_markup(output, "-->", max_bytes, truncated);
            }
            NativeNodeKind::Text(value) => {
                let value = if raw_text {
                    value.clone()
                } else {
                    escape_html_text(value)
                };
                append_bounded_markup(output, &value, max_bytes, truncated);
            }
            NativeNodeKind::Document | NativeNodeKind::DocumentFragment => {
                let children = node
                    .template_content
                    .and_then(|fragment| self.raw_node(fragment))
                    .map_or_else(
                        || node.children().to_vec(),
                        |fragment| fragment.children().to_vec(),
                    );
                for child in children {
                    self.append_serialized_node(child, max_bytes, output, truncated, false);
                    if *truncated {
                        break;
                    }
                }
            }
            NativeNodeKind::DocumentType {
                name,
                public_id,
                system_id,
            } => {
                append_bounded_markup(output, "<!DOCTYPE ", max_bytes, truncated);
                append_bounded_markup(output, name, max_bytes, truncated);
                if let Some(public_id) = public_id {
                    append_bounded_markup(output, " PUBLIC \"", max_bytes, truncated);
                    append_bounded_markup(output, public_id, max_bytes, truncated);
                    append_bounded_markup(output, "\"", max_bytes, truncated);
                    if let Some(system_id) = system_id {
                        append_bounded_markup(output, " \"", max_bytes, truncated);
                        append_bounded_markup(output, system_id, max_bytes, truncated);
                        append_bounded_markup(output, "\"", max_bytes, truncated);
                    }
                } else if let Some(system_id) = system_id {
                    append_bounded_markup(output, " SYSTEM \"", max_bytes, truncated);
                    append_bounded_markup(output, system_id, max_bytes, truncated);
                    append_bounded_markup(output, "\"", max_bytes, truncated);
                }
                append_bounded_markup(output, ">", max_bytes, truncated);
            }
            NativeNodeKind::Element { name, attributes } => {
                append_bounded_markup(output, "<", max_bytes, truncated);
                append_bounded_markup(output, name, max_bytes, truncated);
                for (attribute, value) in attributes {
                    if *truncated {
                        break;
                    }
                    append_bounded_markup(output, " ", max_bytes, truncated);
                    append_bounded_markup(output, attribute, max_bytes, truncated);
                    append_bounded_markup(output, "=\"", max_bytes, truncated);
                    append_bounded_markup(
                        output,
                        &escape_html_attribute(value),
                        max_bytes,
                        truncated,
                    );
                    append_bounded_markup(output, "\"", max_bytes, truncated);
                }
                append_bounded_markup(output, ">", max_bytes, truncated);
                if *truncated
                    || (node.namespace_uri() == Some(HTML_NAMESPACE_URI) && is_void_element(name))
                {
                    return;
                }
                let children = node
                    .template_content
                    .and_then(|fragment| self.raw_node(fragment))
                    .map_or_else(
                        || node.children().to_vec(),
                        |fragment| fragment.children().to_vec(),
                    );
                let child_raw_text = node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                    && is_html_literal_text_element(name);
                for child in children {
                    self.append_serialized_node(
                        child,
                        max_bytes,
                        output,
                        truncated,
                        child_raw_text,
                    );
                    if *truncated {
                        return;
                    }
                }
                append_bounded_markup(output, "</", max_bytes, truncated);
                append_bounded_markup(output, name, max_bytes, truncated);
                append_bounded_markup(output, ">", max_bytes, truncated);
            }
        }
    }

    pub fn title(&self, max_bytes: usize) -> (String, bool) {
        let Some(title_id) = self.find_element(self.root, "title") else {
            return (String::new(), false);
        };
        let mut content = String::new();
        self.collect_raw_text(title_id, &mut content);
        collapse_text(&content, max_bytes)
    }

    /// Return visible text with whitespace collapsed and an explicit truncation
    /// bit. Head, script, style, template, hidden, and aria-hidden subtrees are
    /// omitted until style/layout semantics exist.
    pub fn visible_text(&self, max_bytes: usize) -> (String, bool) {
        let hidden_by_index = self.effective_hidden_by_index();
        self.visible_text_with_hidden(max_bytes, &hidden_by_index)
    }

    pub(crate) fn visible_text_projection_with_counts(
        &self,
        max_bytes: usize,
    ) -> (String, bool, usize, usize) {
        let hidden_by_index = self.effective_hidden_by_index();
        let hidden_nodes = hidden_by_index.values().filter(|hidden| **hidden).count();
        let node_count = hidden_by_index.len();
        let (text, truncated) = self.visible_text_with_hidden(max_bytes, &hidden_by_index);
        (text, truncated, node_count, hidden_nodes)
    }

    pub(super) fn visible_text_with_hidden(
        &self,
        max_bytes: usize,
        hidden_by_index: &std::collections::HashMap<u32, bool>,
    ) -> (String, bool) {
        let mut output = String::new();
        let mut truncated = false;
        self.collect_visible(
            self.root,
            false,
            hidden_by_index,
            &mut output,
            &mut truncated,
            max_bytes,
        );
        (output, truncated)
    }
}

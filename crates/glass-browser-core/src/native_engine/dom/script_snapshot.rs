use super::*;

const MAX_NATIVE_SCRIPT_SNAPSHOT_CACHE_BYTES: usize = 16 * 1024 * 1024;

fn script_snapshot_retained_bytes(snapshot: &NativeScriptDocumentSnapshot) -> usize {
    let mut bytes = snapshot
        .title
        .len()
        .saturating_add(snapshot.visible_text.len());
    for element in &snapshot.elements {
        bytes = bytes
            .saturating_add(element.tag_name.len())
            .saturating_add(element.text.len())
            .saturating_add(element.inner_html.len())
            .saturating_add(element.value.as_ref().map_or(0, String::len))
            .saturating_add(element.image_current_src.len())
            .saturating_add(element.media_current_src.len())
            .saturating_add(element.validation_message.len())
            .saturating_add(element.custom_validity.len());
        bytes = bytes.saturating_add(element.namespace_uri.as_ref().map_or(0, String::len));
        for (name, value) in &element.attributes {
            bytes = bytes.saturating_add(name.len()).saturating_add(value.len());
        }
        for (name, namespace) in &element.attribute_namespaces {
            bytes = bytes
                .saturating_add(name.len())
                .saturating_add(namespace.len());
        }
        for file in &element.files {
            bytes = bytes
                .saturating_add(file.name.len())
                .saturating_add(file.media_type.len())
                .saturating_add(file.bytes.len());
        }
    }
    for node in &snapshot.nodes {
        bytes = bytes
            .saturating_add(node.node_name.len())
            .saturating_add(node.node_value.as_ref().map_or(0, String::len))
            .saturating_add(
                node.children
                    .len()
                    .saturating_mul(std::mem::size_of::<u32>()),
            );
    }
    for resource in &snapshot.canvas_resources {
        bytes = bytes.saturating_add(resource.pixels_base64.len());
    }
    for resource in &snapshot.image_resources {
        bytes = bytes
            .saturating_add(resource.source.len())
            .saturating_add(resource.pixels_base64.len());
        for frame in &resource.frames {
            bytes = bytes.saturating_add(frame.pixels_base64.len());
        }
    }
    let structures = snapshot
        .elements
        .len()
        .saturating_mul(std::mem::size_of::<NativeScriptElementSnapshot>())
        .saturating_add(
            snapshot
                .nodes
                .len()
                .saturating_mul(std::mem::size_of::<NativeScriptNodeSnapshot>()),
        );
    bytes.saturating_mul(2).saturating_add(structures)
}

impl NativeDocument {
    pub(crate) fn script_snapshot_generation(&self) -> u64 {
        self.layout_style_cache.script_snapshot_generation()
    }

    pub(crate) fn element_node_ids(&self) -> impl Iterator<Item = NativeNodeId> + '_ {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
            .filter(|node| node.element_name().is_some())
            .map(NativeNode::id)
    }

    /// Return the embedded browsing-context sources in document order. The
    /// frame owner resolves these references against the current document;
    /// an omitted or empty `src` is the standard initial blank document.
    pub(crate) fn embedded_frame_sources(&self) -> Vec<(NativeNodeId, String, Option<String>)> {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
            .filter(|node| matches!(node.element_name(), Some("iframe" | "frame")))
            .map(|node| {
                let sandbox = node.attribute("sandbox").map(str::to_owned);
                if let Some(srcdoc) = node.attribute("srcdoc") {
                    return (
                        node.id(),
                        format!("data:text/html,{}", encode_data_url_payload(srcdoc)),
                        sandbox,
                    );
                }
                (
                    node.id(),
                    node.attribute("src")
                        .map(str::trim)
                        .filter(|source| !source.is_empty())
                        .unwrap_or("about:blank")
                        .to_owned(),
                    sandbox,
                )
            })
            .collect()
    }

    pub(crate) fn embedded_frame_owner_removal_sequences(&self) -> Vec<(u32, u64)> {
        self.embedded_frame_owner_removal_sequences
            .iter()
            .map(|(node_index, sequence)| (*node_index, *sequence))
            .collect()
    }

    fn script_image_resources(&self, viewport: Viewport) -> Vec<NativeScriptImageResourceSnapshot> {
        let snapshot =
            |node_index, source: String, image: &NativeImage| NativeScriptImageResourceSnapshot {
                node_index,
                source,
                width: image.width,
                height: image.height,
                pixels_base64: base64::engine::general_purpose::STANDARD
                    .encode(image.current_pixels()),
                frames: image
                    .frames
                    .iter()
                    .map(|frame| NativeScriptImageFrameSnapshot {
                        delay_ms: frame.delay_ms,
                        pixels_base64: base64::engine::general_purpose::STANDARD
                            .encode(&frame.pixels),
                    })
                    .collect(),
                loop_count: image.loop_count,
                animation_elapsed_ms: image.animation_elapsed_ms(),
            };
        let mut resources = Vec::new();
        let mut included = BTreeSet::new();
        for (node_index, resource) in &self.image_resources {
            let node_id = NativeNodeId::from_parts(self.generation, *node_index);
            let Some(_node) = self.node(node_id) else {
                continue;
            };
            if !is_image_resource_node_in(&self.nodes, *node_index as usize)
                || self.image_loads.get(node_index) != Some(&resource.source)
            {
                continue;
            }
            resources.push(snapshot(
                *node_index,
                resource.source.clone(),
                &resource.image,
            ));
            included.insert(*node_index);
        }
        for node in &self.nodes {
            let node_index = node.id().index();
            if included.contains(&node_index)
                || !is_image_resource_node_in(&self.nodes, node_index as usize)
            {
                continue;
            }
            let source = self.image_current_src(node.id(), viewport);
            let Some(image) = decode_data_image(&source) else {
                continue;
            };
            resources.push(snapshot(node_index, source, &image));
        }
        resources
    }

    pub(crate) fn script_snapshot_for_viewport(
        &self,
        max_text_bytes: usize,
        viewport: Viewport,
    ) -> NativeScriptDocumentSnapshot {
        if let Some(snapshot) =
            self.layout_style_cache
                .script_snapshot(self.revision, max_text_bytes, viewport)
        {
            let cache_hit_stage = crate::native_engine::content_process::ContentLoadStage::begin(
                0,
                "script_snapshot_cache_hit",
            );
            cache_hit_stage.finish(Some(snapshot.elements.len()));
            return snapshot;
        }
        let text_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "script_snapshot_text_projection",
        );
        let hidden_by_index = self.effective_hidden_by_index();
        let (title, _) = self.title(max_text_bytes);
        let (visible_text, _) = self.visible_text_with_hidden(max_text_bytes, &hidden_by_index);
        text_stage.finish(Some(visible_text.len()));
        let script_node_indices = self
            .script_node_ids
            .values()
            .map(|id| id.index())
            .collect::<BTreeSet<_>>();
        let nodes_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "script_snapshot_nodes",
        );
        let nodes = self
            .nodes
            .iter()
            .filter_map(|node| {
                if self.node(node.id()).is_none()
                    && !script_node_indices.contains(&node.id().index())
                {
                    return None;
                }
                let (node_type, node_name, node_value) = match node.kind() {
                    NativeNodeKind::Document => (9, "#document".to_owned(), None),
                    NativeNodeKind::DocumentFragment => (11, "#document-fragment".to_owned(), None),
                    NativeNodeKind::DocumentType { name, .. } => (10, name.clone(), None),
                    NativeNodeKind::Element { name, .. } => (1, name.to_ascii_uppercase(), None),
                    NativeNodeKind::Comment(value) => {
                        (8, "#comment".to_owned(), Some(value.clone()))
                    }
                    NativeNodeKind::Text(value) => (3, "#text".to_owned(), Some(value.clone())),
                };
                let (public_id, system_id) = match node.kind() {
                    NativeNodeKind::DocumentType {
                        public_id,
                        system_id,
                        ..
                    } => (public_id.clone(), system_id.clone()),
                    _ => (None, None),
                };
                let children = node
                    .children()
                    .iter()
                    .copied()
                    .filter(|child| self.node(*child).is_some())
                    .map(NativeNodeId::index)
                    .collect();
                Some(NativeScriptNodeSnapshot {
                    node_index: node.id().index(),
                    parent_index: node.parent().map(NativeNodeId::index),
                    template_content_index: node.template_content.map(NativeNodeId::index),
                    template_host_index: node.template_host.map(NativeNodeId::index),
                    node_type,
                    node_name,
                    node_value,
                    public_id,
                    system_id,
                    children,
                })
            })
            .collect::<Vec<_>>();
        nodes_stage.finish(Some(nodes.len()));
        let elements_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "script_snapshot_elements",
        );
        let elements = self
            .nodes
            .iter()
            .filter_map(|node| {
                if self.node(node.id()).is_none()
                    && !script_node_indices.contains(&node.id().index())
                {
                    return None;
                }
                let tag_name = node.element_name()?.to_owned();
                let text = if matches!(tag_name.as_str(), "option" | "script") {
                    self.element_text(node.id(), max_text_bytes)
                        .unwrap_or_default()
                        .0
                } else {
                    String::new()
                };
                let (validity, validation_message, will_validate) =
                    self.script_validation_snapshot(node.id());
                let (selection_start, selection_end, selection_direction) = self
                    .selection_snapshot(node.id())
                    .map(|(start, end, direction)| (Some(start), Some(end), Some(direction)))
                    .unwrap_or((None, None, None));
                let (image_complete, image_natural_width, image_natural_height) = self
                    .image_properties(node.id(), viewport)
                    .unwrap_or((false, 0, 0));
                let image_current_src = if tag_name == "img" {
                    self.image_current_src(node.id(), viewport)
                } else {
                    String::new()
                };
                let (
                    media_ready_state,
                    media_network_state,
                    media_duration_millis,
                    media_current_src,
                    media_error,
                    media_video_width,
                    media_video_height,
                ) = self.media_properties(node.id()).unwrap_or((
                    0,
                    0,
                    None,
                    String::new(),
                    None,
                    0,
                    0,
                ));
                let computed_style = self.computed_style_for_layout(node.id());
                Some(NativeScriptElementSnapshot {
                    node_index: node.id().index(),
                    parent_index: self.parent_element_index(node.id()),
                    template_content_index: node.template_content.map(NativeNodeId::index),
                    form_owner_index: self.form_owner(node.id()).map(NativeNodeId::index),
                    tag_name,
                    namespace_uri: node.state.namespace_uri.clone(),
                    attribute_namespaces: node.state.attribute_namespaces.clone(),
                    attributes: node.attributes()?.clone(),
                    text,
                    // The JavaScript host tree already carries each child
                    // identity. Ordinary elements derive text and markup from
                    // those linked children instead of serializing every
                    // descendant again into each ancestor snapshot entry.
                    inner_html: String::new(),
                    value: self.current_value(node.id()),
                    files: node.state.files.clone(),
                    checked: node.state.checked,
                    indeterminate: node.state.indeterminate,
                    selected: node.state.selected,
                    disabled: self.is_disabled(node.id()),
                    hidden: hidden_by_index
                        .get(&node.id().index())
                        .copied()
                        .unwrap_or(false),
                    focused: node.state.focused,
                    focus_anchor_node_index: node.state.focus_anchor_node_index,
                    hovered: node.state.hovered,
                    user_interacted: node.state.user_interacted,
                    validity,
                    validation_message,
                    custom_validity: node.state.custom_validity.clone(),
                    will_validate,
                    selection_start,
                    selection_end,
                    selection_direction,
                    image_complete,
                    image_natural_width,
                    image_natural_height,
                    image_current_src,
                    media_ready_state,
                    media_network_state,
                    media_current_src,
                    media_duration_millis,
                    media_video_width,
                    media_video_height,
                    media_error,
                    scroll_x: 0,
                    scroll_y: 0,
                    inline_style_allowed: node.inline_style_allowed(),
                    computed_style,
                })
            })
            .collect::<Vec<_>>();
        elements_stage.finish(Some(elements.len()));
        let resources_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "script_snapshot_resources",
        );
        let canvas_resources: Vec<NativeCanvasResourceWire> = self
            .canvas_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                (node.element_name() == Some("canvas")).then(|| NativeCanvasResourceWire {
                    node_index: *node_index,
                    width: resource.width,
                    height: resource.height,
                    pixels_base64: base64::engine::general_purpose::STANDARD
                        .encode(&resource.pixels),
                    origin_clean: resource.origin_clean,
                })
            })
            .collect();
        let image_resources = self.script_image_resources(viewport);
        resources_stage.finish(Some(canvas_resources.len() + image_resources.len()));
        let snapshot = NativeScriptDocumentSnapshot {
            revision: self.revision,
            title,
            base_url: String::new(),
            base_fallback_url: String::new(),
            visible_text,
            elements,
            nodes,
            geometry: Vec::new(),
            scroll_x: 0,
            scroll_y: 0,
            scroll_width: 0,
            scroll_height: 0,
            script_nodes: self
                .script_node_ids
                .iter()
                .filter(|(_, id)| id.generation == self.generation)
                .map(|(temporary_index, id)| NativeScriptNodeIdentity {
                    temporary_index: *temporary_index,
                    node_index: id.index,
                })
                .collect(),
            canvas_resources,
            image_resources,
        };
        let has_animated_images = snapshot
            .image_resources
            .iter()
            .any(|resource| resource.frames.len() > 1);
        if !has_animated_images
            && script_snapshot_retained_bytes(&snapshot) <= MAX_NATIVE_SCRIPT_SNAPSHOT_CACHE_BYTES
        {
            self.layout_style_cache.cache_script_snapshot(
                self.revision,
                max_text_bytes,
                viewport,
                snapshot.clone(),
            );
        }
        snapshot
    }

    pub(crate) fn script_snapshot_with_layout(
        &self,
        max_text_bytes: usize,
        viewport: Viewport,
        scroll_offset: NativePoint,
        nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
    ) -> Result<NativeScriptDocumentSnapshot, NativeEngineError> {
        let mut snapshot = self.script_snapshot_for_viewport(max_text_bytes, viewport);
        let layout_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "script_snapshot_layout_compute",
        );
        let cached_projection = self.layout_style_cache.scrolled_script_projection(
            self.revision,
            viewport,
            scroll_offset,
            nested_scroll_offsets,
        );
        let (layout, geometry) = if let Some((layout, geometry)) = cached_projection {
            let cache_hit = crate::native_engine::content_process::ContentLoadStage::begin(
                0,
                "script_snapshot_scroll_projection_cache_hit",
            );
            cache_hit.finish(Some(geometry.len()));
            (layout, geometry)
        } else {
            let layout = self.layout(viewport)?.with_scroll_offset(
                self,
                scroll_offset,
                nested_scroll_offsets,
            )?;
            let geometry_stage = crate::native_engine::content_process::ContentLoadStage::begin(
                0,
                "script_snapshot_geometry_projection",
            );
            let geometry = layout
                .boxes
                .iter()
                .map(|layout_box| {
                    let scroll_container = layout.scroll_container_for(layout_box.node_id);
                    let nested_scroll_offset = if layout_box.fixed {
                        NativePoint { x: 0, y: 0 }
                    } else {
                        layout.nested_scroll_offset_for(self, layout_box.node_id, false)
                    };
                    let is_root = self
                        .node(layout_box.node_id)
                        .and_then(|node| node.element_name())
                        == Some("html");
                    let client_width = if is_root {
                        viewport.width
                    } else {
                        scroll_container
                            .map(|container| container.client_width)
                            .unwrap_or(layout_box.rect.width)
                    };
                    let client_height = if is_root {
                        viewport.height
                    } else {
                        scroll_container
                            .map(|container| container.client_height)
                            .unwrap_or(layout_box.rect.height)
                    };
                    NativeScriptGeometrySnapshot {
                        node_index: layout_box.node_id.index(),
                        x: i64::from(layout_box.rect.x)
                            - i64::from(layout.scroll_offset.x)
                            - i64::from(nested_scroll_offset.x),
                        y: i64::from(layout_box.rect.y)
                            - i64::from(layout.scroll_offset.y)
                            - i64::from(nested_scroll_offset.y),
                        width: layout_box.rect.width,
                        height: layout_box.rect.height,
                        content_x: i64::from(layout_box.content_rect.x)
                            - i64::from(layout.scroll_offset.x)
                            - i64::from(nested_scroll_offset.x),
                        content_y: i64::from(layout_box.content_rect.y)
                            - i64::from(layout.scroll_offset.y)
                            - i64::from(nested_scroll_offset.y),
                        content_width: layout_box.content_rect.width,
                        content_height: layout_box.content_rect.height,
                        scroll_x: if is_root {
                            layout.scroll_offset.x
                        } else {
                            scroll_container
                                .map(|container| container.scroll_offset.x)
                                .unwrap_or(0)
                        },
                        scroll_y: if is_root {
                            layout.scroll_offset.y
                        } else {
                            scroll_container
                                .map(|container| container.scroll_offset.y)
                                .unwrap_or(0)
                        },
                        scroll_width: if is_root {
                            layout.content_width
                        } else {
                            scroll_container
                                .map(|container| container.scroll_width)
                                .unwrap_or(layout_box.rect.width)
                        },
                        scroll_height: if is_root {
                            layout.content_height
                        } else {
                            scroll_container
                                .map(|container| container.scroll_height)
                                .unwrap_or(layout_box.rect.height)
                        },
                        client_width,
                        client_height,
                    }
                })
                .collect::<Vec<_>>();
            geometry_stage.finish(Some(geometry.len()));
            self.layout_style_cache.cache_scrolled_script_projection(
                self.revision,
                viewport,
                scroll_offset,
                nested_scroll_offsets,
                layout.clone(),
                geometry.clone(),
            );
            (layout, geometry)
        };
        layout_stage.finish(Some(layout.boxes.len()));
        snapshot.geometry = geometry;
        let geometry_by_node = snapshot
            .geometry
            .iter()
            .map(|geometry| (geometry.node_index, geometry))
            .collect::<std::collections::HashMap<_, _>>();
        for element in &mut snapshot.elements {
            if let Some(geometry) = geometry_by_node.get(&element.node_index) {
                element.scroll_x = geometry.scroll_x;
                element.scroll_y = geometry.scroll_y;
            }
        }
        snapshot.scroll_x = layout.scroll_offset.x;
        snapshot.scroll_y = layout.scroll_offset.y;
        snapshot.scroll_width = layout.content_width;
        snapshot.scroll_height = layout.content_height;
        Ok(snapshot)
    }

    pub(crate) fn page_script_sources(
        &self,
        max_scripts: usize,
        max_source_bytes: usize,
    ) -> Vec<NativePageScriptSource> {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
            .filter(|node| node.element_name() == Some("script"))
            .filter_map(|node| {
                let module = match node.attribute("type") {
                    None | Some("") => false,
                    Some(value) if value.eq_ignore_ascii_case("module") => true,
                    Some(value) if javascript_mime_type_essence_match(value) => false,
                    Some(_) => return None,
                };
                let external = node
                    .attribute("src")
                    .is_some_and(|source| !source.is_empty());
                let timing = if external && node.attribute("async").is_some() {
                    NativePageScriptTiming::Async
                } else if module || (external && node.attribute("defer").is_some()) {
                    NativePageScriptTiming::Defer
                } else {
                    NativePageScriptTiming::ParserBlocking
                };
                Some((node, module, timing))
            })
            .take(max_scripts)
            .filter_map(|(node, module, timing)| {
                if let Some(source) = node.attribute("src") {
                    return (!source.is_empty()).then(|| {
                        if module {
                            NativePageScriptSource::ModuleExternal {
                                href: source.to_owned(),
                                timing,
                                node_index: node.id().index(),
                                nonce: node.attribute("nonce").map(str::to_owned),
                                integrity: node.attribute("integrity").map(str::to_owned),
                                crossorigin: node.attribute("crossorigin").map(str::to_owned),
                                referrer_policy: self.script_referrer_policy_for_node(node.id()),
                                parser_inserted: true,
                            }
                        } else {
                            NativePageScriptSource::External {
                                href: source.to_owned(),
                                timing,
                                node_index: node.id().index(),
                                nonce: node.attribute("nonce").map(str::to_owned),
                                integrity: node.attribute("integrity").map(str::to_owned),
                                crossorigin: node.attribute("crossorigin").map(str::to_owned),
                                referrer_policy: self.script_referrer_policy_for_node(node.id()),
                                parser_inserted: true,
                            }
                        }
                    });
                }
                let mut source = String::new();
                self.collect_raw_text(node.id(), &mut source);
                (source.len() <= max_source_bytes && !source.is_empty()).then_some(if module {
                    NativePageScriptSource::ModuleInline {
                        source,
                        timing,
                        node_index: node.id().index(),
                        nonce: node.attribute("nonce").map(str::to_owned),
                        referrer_policy: self.script_referrer_policy_for_node(node.id()),
                        parser_inserted: true,
                    }
                } else {
                    NativePageScriptSource::Inline {
                        source,
                        timing,
                        node_index: node.id().index(),
                        nonce: node.attribute("nonce").map(str::to_owned),
                        parser_inserted: true,
                    }
                })
            })
            .collect()
    }

    pub(crate) fn page_import_map_sources(
        &self,
        max_maps: usize,
        max_source_bytes: usize,
    ) -> Vec<NativePageImportMapSource> {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
            .filter(|node| node.element_name() == Some("script"))
            .filter(|node| node.namespace_uri() == Some(HTML_NAMESPACE_URI))
            .filter(|node| {
                node.attribute("type")
                    .is_some_and(|value| value.eq_ignore_ascii_case("importmap"))
            })
            .take(max_maps)
            .map(|node| {
                let source = if node.attribute("src").is_some() {
                    None
                } else {
                    let mut source = String::new();
                    self.collect_raw_text(node.id(), &mut source);
                    (source.len() <= max_source_bytes).then_some(source)
                };
                NativePageImportMapSource {
                    source,
                    node_index: node.id().index(),
                    nonce: node.attribute("nonce").map(str::to_owned),
                }
            })
            .collect()
    }

    /// Mark every parser-discovered script as started before page lifecycle
    /// events run. A script element is single-shot even when its text or
    /// parent is later changed by script.
    pub(crate) fn mark_attached_scripts_started(&mut self) {
        let started = self
            .nodes
            .iter()
            .filter(|node| self.is_attached(node.id()) && node.element_name() == Some("script"))
            .map(|node| node.id().index())
            .collect::<Vec<_>>();
        self.started_script_nodes.extend(started);
    }

    /// Commit the script-start side effect for nodes touched by one host
    /// mutation and return the newly attached page-script sources. Temporary
    /// script nodes may be created in one evaluation and attached in a later
    /// evaluation, so both creation and insertion commands are considered.
    pub(crate) fn take_newly_attached_page_script_sources(
        &mut self,
        commands: &[NativeScriptCommand],
        max_scripts: usize,
        max_source_bytes: usize,
    ) -> Vec<NativePageScriptSource> {
        let mut roots = Vec::new();
        let mut attachment_roots = Vec::new();
        let mut realm_started = BTreeSet::new();
        for command in commands {
            let node_index = match command {
                NativeScriptCommand::CreateElement {
                    node_index,
                    tag_name,
                    ..
                } if tag_name.eq_ignore_ascii_case("script") => Some(*node_index),
                NativeScriptCommand::AppendChild { child_index, .. }
                | NativeScriptCommand::InsertBefore { child_index, .. } => {
                    attachment_roots.push(*child_index);
                    Some(*child_index)
                }
                NativeScriptCommand::StartScript { node_index } => {
                    let id = self
                        .script_node_ids
                        .get(node_index)
                        .copied()
                        .unwrap_or_else(|| NativeNodeId::from_parts(self.generation, *node_index));
                    realm_started.insert(id.index());
                    Some(*node_index)
                }
                _ => None,
            };
            let Some(node_index) = node_index else {
                continue;
            };
            let id = self
                .script_node_ids
                .get(&node_index)
                .copied()
                .unwrap_or_else(|| NativeNodeId::from_parts(self.generation, node_index));
            if !roots.contains(&id) {
                roots.push(id);
            }
        }

        let mut candidate_nodes = BTreeSet::new();
        for root in roots.iter().copied() {
            self.collect_attached_script_nodes(root, &mut candidate_nodes);
        }
        if candidate_nodes.is_empty() {
            return Vec::new();
        }

        let mut attachment_order = Vec::new();
        let mut ordered_nodes = BTreeSet::new();
        for root in attachment_roots.iter().copied() {
            let id = self
                .script_node_ids
                .get(&root)
                .copied()
                .unwrap_or_else(|| NativeNodeId::from_parts(self.generation, root));
            self.collect_attached_script_nodes_in_order(
                id,
                &mut attachment_order,
                &mut ordered_nodes,
            );
        }
        for root in roots {
            self.collect_attached_script_nodes_in_order(
                root,
                &mut attachment_order,
                &mut ordered_nodes,
            );
        }
        let source_order = attachment_order
            .into_iter()
            .enumerate()
            .map(|(order, node_index)| (node_index, order))
            .collect::<BTreeMap<_, _>>();

        let mut sources = self
            .page_script_sources(self.nodes.len(), max_source_bytes)
            .into_iter()
            .filter(|source| {
                let node_index = source.node_index();
                candidate_nodes.contains(&node_index)
                    && !self.started_script_nodes.contains(&node_index)
                    && !realm_started.contains(&node_index)
            })
            .collect::<Vec<_>>();
        sources.extend(
            self.page_import_map_sources(self.nodes.len(), max_source_bytes)
                .into_iter()
                .filter(|source| {
                    candidate_nodes.contains(&source.node_index)
                        && !self.started_script_nodes.contains(&source.node_index)
                        && !realm_started.contains(&source.node_index)
                })
                .map(|source| NativePageScriptSource::ImportMap {
                    source: source.source,
                    node_index: source.node_index,
                    nonce: source.nonce,
                }),
        );
        sources.sort_by_key(|source| {
            let node_index = source.node_index();
            (
                source_order.get(&node_index).copied().unwrap_or(usize::MAX),
                node_index,
            )
        });
        sources.truncate(max_scripts);
        let sources = sources
            .into_iter()
            .map(NativePageScriptSource::into_dynamic)
            .collect();
        self.started_script_nodes.extend(candidate_nodes);
        sources
    }

    fn collect_attached_script_nodes(&self, id: NativeNodeId, output: &mut BTreeSet<u32>) {
        if !self.is_attached(id) {
            return;
        }
        let Some(node) = self.node(id) else {
            return;
        };
        if node.element_name() == Some("script") {
            output.insert(id.index());
        }
        for child in node.children() {
            self.collect_attached_script_nodes(*child, output);
        }
    }

    fn collect_attached_script_nodes_in_order(
        &self,
        id: NativeNodeId,
        output: &mut Vec<u32>,
        seen: &mut BTreeSet<u32>,
    ) {
        if !self.is_attached(id) {
            return;
        }
        let Some(node) = self.node(id) else {
            return;
        };
        if node.element_name() == Some("script") && seen.insert(id.index()) {
            output.push(id.index());
        }
        for child in node.children() {
            self.collect_attached_script_nodes_in_order(*child, output, seen);
        }
    }
}

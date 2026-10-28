use super::*;
use crate::native_engine::css;
use flate2::{Compression, write::ZlibEncoder};
use std::io::Write;

fn encode_content_image_pixels(pixels: &[u8]) -> (String, String) {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    if encoder.write_all(pixels).is_ok()
        && let Ok(compressed) = encoder.finish()
        && compressed.len() < pixels.len()
    {
        return (
            String::new(),
            base64::engine::general_purpose::STANDARD.encode(compressed),
        );
    }
    (
        base64::engine::general_purpose::STANDARD.encode(pixels),
        String::new(),
    )
}

impl NativeDocument {
    pub(crate) fn to_content_wire(&self) -> NativeDocumentWire {
        let style_stage = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").map(|_| {
            crate::native_engine::content_process::ContentLoadStage::begin(
                0,
                "document_wire_computed_styles",
            )
        });
        let mut computed_style_table = Vec::new();
        let mut computed_style_indices = Vec::with_capacity(self.nodes.len());
        let mut computed_style_lookup = HashMap::<Vec<u8>, u32>::new();
        for index in 0..self.nodes.len() {
            let style = self.computed_style_for_layout(NativeNodeId {
                generation: self.generation,
                index: u32::try_from(index).unwrap_or(u32::MAX),
            });
            let style_key = serde_json::to_vec(&style)
                .unwrap_or_else(|_| format!("serialization-error:{style:?}").into_bytes());
            let style_index = if let Some(existing) = computed_style_lookup.get(&style_key) {
                *existing
            } else {
                let style_index = u32::try_from(computed_style_table.len()).unwrap_or(u32::MAX);
                computed_style_table.push(style);
                computed_style_lookup.insert(style_key, style_index);
                style_index
            };
            computed_style_indices.push(style_index);
        }
        if let Some(stage) = style_stage {
            stage.finish(Some(self.nodes.len()));
        }
        let node_stage = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").map(|_| {
            crate::native_engine::content_process::ContentLoadStage::begin(0, "document_wire_nodes")
        });
        let nodes = self
            .nodes
            .iter()
            .map(|node| NativeNodeWire {
                parent: node.parent.map(|parent| parent.index),
                children: node.children.iter().map(|child| child.index).collect(),
                template_content: node.template_content.map(|fragment| fragment.index),
                template_host: node.template_host.map(|host| host.index),
                shadow_host: node.shadow_host.map(|host| host.index),
                shadow_root: node.shadow_root.map(|root| root.index),
                shadow_mode: node.shadow_mode.clone(),
                kind: match &node.kind {
                    NativeNodeKind::Document => NativeNodeKindWire::Document,
                    NativeNodeKind::DocumentFragment => NativeNodeKindWire::DocumentFragment,
                    NativeNodeKind::Element { name, attributes } => NativeNodeKindWire::Element {
                        name: name.clone(),
                        attributes: attributes.clone(),
                    },
                    NativeNodeKind::DocumentType {
                        name,
                        public_id,
                        system_id,
                    } => NativeNodeKindWire::DocumentType {
                        name: name.clone(),
                        public_id: public_id.clone(),
                        system_id: system_id.clone(),
                    },
                    NativeNodeKind::Comment(value) => NativeNodeKindWire::Comment(value.clone()),
                    NativeNodeKind::Text(value) => NativeNodeKindWire::Text(value.clone()),
                },
                state: NativeElementStateWire {
                    namespace_uri: node.element_name().and_then(|_| {
                        match node.state.namespace_uri.as_deref() {
                            Some(super::HTML_NAMESPACE_URI) => None,
                            Some(namespace) => Some(namespace.to_owned()),
                            None => Some(String::new()),
                        }
                    }),
                    value: node.state.value.clone(),
                    files: node.state.files.clone(),
                    attribute_namespaces: node.state.attribute_namespaces.clone(),
                    checked: node.state.checked,
                    indeterminate: node.state.indeterminate,
                    focused: node.state.focused,
                    focus_anchor_node_index: node.state.focus_anchor_node_index,
                    hovered: node.state.hovered,
                    user_interacted: node.state.user_interacted,
                    selected: node.state.selected,
                    form_associated_disabled: node.state.form_associated_disabled,
                    custom_validity: node.state.custom_validity.clone(),
                    selection_start: node.state.selection_start,
                    selection_end: node.state.selection_end,
                    selection_direction: node.state.selection_direction.clone(),
                },
            })
            .collect();
        if let Some(stage) = node_stage {
            stage.finish(Some(self.nodes.len()));
        }
        let resources_stage = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").map(|_| {
            crate::native_engine::content_process::ContentLoadStage::begin(
                0,
                "document_wire_resources",
            )
        });
        let image_resources = self
            .image_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let _node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                (is_image_resource_node_in(&self.nodes, *node_index as usize)
                    && self.image_loads.get(node_index) == Some(&resource.source))
                .then(|| {
                    let (pixels_base64, pixels_zlib_base64) =
                        encode_content_image_pixels(&resource.image.pixels);
                    NativeImageResourceWire {
                        node_index: *node_index,
                        source: resource.source.clone(),
                        width: resource.image.width,
                        height: resource.image.height,
                        pixels_base64,
                        pixels_zlib_base64,
                        frames: resource
                            .image
                            .frames
                            .iter()
                            .map(|frame| {
                                let (pixels_base64, pixels_zlib_base64) =
                                    encode_content_image_pixels(&frame.pixels);
                                NativeImageFrameWire {
                                    delay_ms: frame.delay_ms,
                                    pixels_base64,
                                    pixels_zlib_base64,
                                }
                            })
                            .collect(),
                        loop_count: resource.image.loop_count,
                    }
                })
            })
            .collect();
        let image_loads = self
            .image_loads
            .iter()
            .filter_map(|(node_index, source)| {
                let _node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                is_image_resource_node_in(&self.nodes, *node_index as usize).then(|| {
                    NativeImageLoadWire {
                        node_index: *node_index,
                        source: source.clone(),
                    }
                })
            })
            .collect();
        let video_poster_resources = self
            .video_poster_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                (self.selected_video_poster_source(node_id).as_deref()
                    == Some(resource.source.as_str())
                    && self.video_poster_loads.get(node_index) == Some(&resource.source))
                .then(|| {
                    let (pixels_base64, pixels_zlib_base64) =
                        encode_content_image_pixels(&resource.image.pixels);
                    NativeImageResourceWire {
                        node_index: *node_index,
                        source: resource.source.clone(),
                        width: resource.image.width,
                        height: resource.image.height,
                        pixels_base64,
                        pixels_zlib_base64,
                        frames: resource
                            .image
                            .frames
                            .iter()
                            .map(|frame| {
                                let (pixels_base64, pixels_zlib_base64) =
                                    encode_content_image_pixels(&frame.pixels);
                                NativeImageFrameWire {
                                    delay_ms: frame.delay_ms,
                                    pixels_base64,
                                    pixels_zlib_base64,
                                }
                            })
                            .collect(),
                        loop_count: resource.image.loop_count,
                    }
                })
            })
            .collect();
        let video_poster_loads = self
            .video_poster_loads
            .iter()
            .filter_map(|(node_index, source)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                (self.selected_video_poster_source(node_id).as_deref() == Some(source.as_str()))
                    .then(|| NativeImageLoadWire {
                        node_index: *node_index,
                        source: source.clone(),
                    })
            })
            .collect();
        let media_resources =
            self.media_resources
                .iter()
                .filter_map(|(node_index, resource)| {
                    let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                    ((node.element_name() == Some("audio") || node.element_name() == Some("video"))
                        && self.media_loads.get(node_index) == Some(&resource.source))
                    .then(|| NativeMediaResourceWire {
                        node_index: *node_index,
                        source: resource.source.clone(),
                        content_type: resource.content_type.clone(),
                        byte_length: resource.byte_length,
                        duration_millis: resource.duration_millis,
                        video_frames: resource.video_frames.clone(),
                        audio_track: resource.audio_track.as_ref().map(|track| {
                            NativeAudioTrackWire {
                                channels: track.channels,
                                sample_rate: track.sample_rate,
                                pcm_s16le_base64: base64::engine::general_purpose::STANDARD
                                    .encode(&track.pcm_s16le),
                            }
                        }),
                        current_time_millis: resource.current_time_millis,
                        is_playing: resource.is_playing,
                        playback_rate_millionths: resource.playback_rate_millionths,
                    })
                })
                .collect();
        let media_loads =
            self.media_loads
                .iter()
                .filter_map(|(node_index, source)| {
                    let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                    (node.element_name() == Some("audio") || node.element_name() == Some("video"))
                        .then(|| NativeMediaLoadWire {
                            node_index: *node_index,
                            source: source.clone(),
                        })
                })
                .collect();
        let media_errors =
            self.media_errors
                .iter()
                .filter_map(|(node_index, source)| {
                    let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                    (node.element_name() == Some("audio") || node.element_name() == Some("video"))
                        .then(|| NativeMediaLoadWire {
                            node_index: *node_index,
                            source: source.clone(),
                        })
                })
                .collect();
        let canvas_resources = self
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
        let background_image_sources = self
            .background_image_sources
            .iter()
            .map(|(source_id, source)| NativeBackgroundImageSourceWire {
                source_id: *source_id,
                source: source.clone(),
            })
            .collect();
        let background_image_resources = self
            .background_image_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                (self.background_image_source_for_node(node_id) == Some(resource.source.as_str()))
                    .then(|| {
                        let (pixels_base64, pixels_zlib_base64) =
                            encode_content_image_pixels(&resource.image.pixels);
                        NativeImageResourceWire {
                            node_index: *node_index,
                            source: resource.source.clone(),
                            width: resource.image.width,
                            height: resource.image.height,
                            pixels_base64,
                            pixels_zlib_base64,
                            frames: resource
                                .image
                                .frames
                                .iter()
                                .map(|frame| {
                                    let (pixels_base64, pixels_zlib_base64) =
                                        encode_content_image_pixels(&frame.pixels);
                                    NativeImageFrameWire {
                                        delay_ms: frame.delay_ms,
                                        pixels_base64,
                                        pixels_zlib_base64,
                                    }
                                })
                                .collect(),
                            loop_count: resource.image.loop_count,
                        }
                    })
            })
            .collect();
        let font_resources = self
            .font_resources
            .iter()
            .map(|resource| NativeFontFaceResourceWire {
                family: resource.family.clone(),
                family_key: resource.family_key,
                weight: FontWeightValue::from_numeric(resource.weight.nominal())
                    .unwrap_or(FontWeightValue::Normal),
                weight_range: (resource.weight.min != resource.weight.max)
                    .then_some(resource.weight),
                style: resource.style,
                stretch: resource.stretch,
                unicode_ranges: resource.unicode_ranges.clone(),
                variation_settings: resource.variation_settings,
                feature_settings: resource.feature_settings,
                size_adjust: resource.size_adjust,
                metric_overrides: resource.metric_overrides,
                font_display: resource.font_display,
                data_base64: base64::engine::general_purpose::STANDARD
                    .encode(resource.bytes.as_ref()),
            })
            .collect();
        let wire = NativeDocumentWire {
            native_document_state_replayable: false,
            nodes,
            computed_styles: Vec::new(),
            computed_style_table,
            computed_style_indices,
            viewport: self.viewport,
            quirks_mode: self.quirks_mode,
            document_referrer_policy: self.document_referrer_policy.as_str().to_owned(),
            inherited_base_url: self.inherited_base_url.clone(),
            font_resources,
            blocked_inline_style_nodes: self
                .nodes
                .iter()
                .filter(|node| {
                    (node.element_name() == Some("style") || node.attribute("style").is_some())
                        && !node.inline_style_allowed()
                })
                .map(|node| node.id().index())
                .collect(),
            script_nodes: self
                .script_node_ids
                .iter()
                .filter(|(_, id)| id.generation == self.generation)
                .map(|(temporary_index, id)| NativeScriptNodeIdentity {
                    temporary_index: *temporary_index,
                    node_index: id.index,
                })
                .collect(),
            started_script_nodes: self.started_script_nodes.iter().copied().collect(),
            embedded_frame_owner_removal_sequences: self
                .embedded_frame_owner_removal_sequences
                .clone(),
            next_embedded_frame_owner_removal_sequence: self
                .next_embedded_frame_owner_removal_sequence,
            image_resources,
            background_image_sources,
            background_image_resources,
            image_loads,
            video_poster_resources,
            video_poster_loads,
            canvas_resources,
            media_resources,
            media_loads,
            media_errors,
        };
        if let Some(stage) = resources_stage {
            stage.finish(Some(self.nodes.len()));
        }
        wire
    }

    pub(crate) fn from_content_wire(
        mut wire: NativeDocumentWire,
        limits: &NativeEngineLimits,
        generation: u32,
    ) -> Result<Self, NativeEngineError> {
        if wire.native_document_state_replayable {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "unchanged content-process document cannot be decoded as a snapshot".into(),
            });
        }
        if !wire.computed_style_table.is_empty() || !wire.computed_style_indices.is_empty() {
            if !wire.computed_styles.is_empty()
                || wire.computed_style_table.len() > wire.nodes.len()
                || wire.computed_style_indices.len() != wire.nodes.len()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned invalid interned computed styles".into(),
                });
            }
            let mut expanded = Vec::with_capacity(wire.nodes.len());
            for index in &wire.computed_style_indices {
                let style = wire
                    .computed_style_table
                    .get(*index as usize)
                    .ok_or_else(|| NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned an invalid computed-style index".into(),
                    })?;
                expanded.push(*style);
            }
            wire.computed_styles = expanded;
        }
        let viewport = wire.viewport;
        let quirks_mode = wire.quirks_mode;
        let document_referrer_policy =
            NativeFetchReferrerPolicy::parse(&wire.document_referrer_policy).map_err(|_| {
                NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid Document referrer policy".into(),
                }
            })?;
        let inherited_base_url = wire.inherited_base_url.take();
        let embedded_frame_owner_removal_sequences = wire.embedded_frame_owner_removal_sequences;
        let next_embedded_frame_owner_removal_sequence =
            wire.next_embedded_frame_owner_removal_sequence;
        viewport.validate()?;
        limits.validate()?;
        if wire.nodes.is_empty() {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an empty document snapshot".into(),
            });
        }
        if wire.nodes.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process DOM nodes",
                limits.max_nodes,
                wire.nodes.len(),
            ));
        }
        if wire.computed_styles.len() != wire.nodes.len() {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned incomplete computed styles".into(),
            });
        }
        if wire.font_resources.len() > MAX_NATIVE_FONT_FACES {
            return Err(NativeEngineError::limit(
                "content-process font faces",
                MAX_NATIVE_FONT_FACES,
                wire.font_resources.len(),
            ));
        }
        let max_encoded_font_bytes =
            (MAX_NATIVE_FONT_BYTES.saturating_add(2).saturating_div(3)).saturating_mul(4);
        let mut font_resources = Vec::with_capacity(wire.font_resources.len());
        let mut total_font_bytes = 0usize;
        for resource in wire.font_resources {
            let weight = resource
                .weight_range
                .unwrap_or_else(|| NativeFontWeightRange::singleton(resource.weight));
            if resource.family.is_empty()
                || resource.family.len() > MAX_ATTRIBUTE_BYTES
                || resource.family.bytes().any(|byte| byte.is_ascii_control())
                || resource.family_key != css::font_family_hash(&resource.family)
                || weight.min < 1
                || weight.min > weight.max
                || weight.max > 1000
                || resource.stretch.min < 500
                || resource.stretch.min > resource.stretch.max
                || resource.stretch.max > 2000
                || !(250..=4000).contains(&resource.size_adjust)
                || resource.unicode_ranges.len() > css::MAX_NATIVE_FONT_FACE_UNICODE_RANGES
                || resource
                    .unicode_ranges
                    .iter()
                    .any(|range| range.start > range.end || range.end > 0x10_FFFF)
                || !resource.variation_settings.is_valid()
                || !resource.feature_settings.is_valid()
                || resource
                    .metric_overrides
                    .ascent
                    .is_some_and(|value| value > 10_000)
                || resource
                    .metric_overrides
                    .descent
                    .is_some_and(|value| value > 10_000)
                || resource
                    .metric_overrides
                    .line_gap
                    .is_some_and(|value| value > 10_000)
                || resource.data_base64.len() > max_encoded_font_bytes
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned invalid font metadata".into(),
                });
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&resource.data_base64)
                .map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned invalid font bytes".into(),
                })?;
            if bytes.is_empty() || bytes.len() > MAX_NATIVE_FONT_BYTES {
                return Err(NativeEngineError::limit(
                    "content-process font bytes",
                    MAX_NATIVE_FONT_BYTES,
                    bytes.len(),
                ));
            }
            total_font_bytes = total_font_bytes.saturating_add(bytes.len());
            if total_font_bytes > MAX_NATIVE_FONT_TOTAL_BYTES {
                return Err(NativeEngineError::limit(
                    "content-process font bytes",
                    MAX_NATIVE_FONT_TOTAL_BYTES,
                    total_font_bytes,
                ));
            }
            font_resources.push(NativeFontFaceResource {
                family: resource.family,
                family_key: resource.family_key,
                weight,
                style: resource.style,
                stretch: resource.stretch,
                size_adjust: resource.size_adjust,
                metric_overrides: resource.metric_overrides,
                font_display: resource.font_display,
                unicode_ranges: resource.unicode_ranges,
                variation_settings: resource.variation_settings,
                feature_settings: resource.feature_settings,
                bytes: Arc::from(bytes),
            });
        }
        let font_book = NativeFontBook::from_resources(&font_resources);
        if wire.background_image_sources.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process background image sources",
                limits.max_nodes,
                wire.background_image_sources.len(),
            ));
        }
        let mut background_image_sources = BTreeMap::new();
        for source in &wire.background_image_sources {
            if source.source_id == 0
                || source.source.is_empty()
                || source.source.len() > MAX_ATTRIBUTE_BYTES
                || source.source.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid background image source".into(),
                });
            }
            if background_image_sources
                .insert(source.source_id, source.source.clone())
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate background image sources".into(),
                });
            }
        }
        if wire.computed_styles.iter().any(|style| {
            style
                .background_image()
                .is_some_and(|source_id| !background_image_sources.contains_key(&source_id))
        }) {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an unknown background image source".into(),
            });
        }
        if wire.blocked_inline_style_nodes.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process blocked inline styles",
                limits.max_nodes,
                wire.blocked_inline_style_nodes.len(),
            ));
        }
        let blocked_inline_style_nodes = wire
            .blocked_inline_style_nodes
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if blocked_inline_style_nodes.len() != wire.blocked_inline_style_nodes.len()
            || blocked_inline_style_nodes.iter().any(|node_index| {
                let Ok(index) = usize::try_from(*node_index) else {
                    return true;
                };
                let Some(node) = wire.nodes.get(index) else {
                    return true;
                };
                match &node.kind {
                    NativeNodeKindWire::Element { name, attributes } => {
                        !name.eq_ignore_ascii_case("style")
                            && !attributes
                                .iter()
                                .any(|(name, _)| name.eq_ignore_ascii_case("style"))
                    }
                    NativeNodeKindWire::Document
                    | NativeNodeKindWire::DocumentFragment
                    | NativeNodeKindWire::DocumentType { .. }
                    | NativeNodeKindWire::Comment(_)
                    | NativeNodeKindWire::Text(_) => true,
                }
            })
        {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an invalid blocked inline style node".into(),
            });
        }
        let node_id = |index: u32| -> Result<NativeNodeId, NativeEngineError> {
            let index_usize = usize::try_from(index).map_err(|_| NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an invalid node index".into(),
            })?;
            if index_usize >= wire.nodes.len() {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range node link".into(),
                });
            }
            Ok(NativeNodeId { generation, index })
        };
        let mut nodes = Vec::with_capacity(wire.nodes.len());
        for (index, wire_node) in wire.nodes.iter().enumerate() {
            let index = u32::try_from(index).map_err(|_| {
                NativeEngineError::limit("content-process DOM node index", u32::MAX as usize, index)
            })?;
            let parent = wire_node.parent.map(node_id).transpose()?;
            let children = wire_node
                .children
                .iter()
                .copied()
                .map(node_id)
                .collect::<Result<Vec<_>, _>>()?;
            let template_content = wire_node.template_content.map(node_id).transpose()?;
            let template_host = wire_node.template_host.map(node_id).transpose()?;
            let kind = match &wire_node.kind {
                NativeNodeKindWire::Document => NativeNodeKind::Document,
                NativeNodeKindWire::DocumentFragment => NativeNodeKind::DocumentFragment,
                NativeNodeKindWire::Element { name, attributes } => {
                    if name.len() > MAX_ATTRIBUTE_BYTES
                        || attributes.iter().any(|(name, value)| {
                            name.len() > MAX_ATTRIBUTE_BYTES || value.len() > MAX_ATTRIBUTE_BYTES
                        })
                    {
                        return Err(NativeEngineError::limit(
                            "content-process attribute",
                            MAX_ATTRIBUTE_BYTES,
                            MAX_ATTRIBUTE_BYTES.saturating_add(1),
                        ));
                    }
                    NativeNodeKind::Element {
                        name: name.clone(),
                        attributes: attributes.clone(),
                    }
                }
                NativeNodeKindWire::DocumentType {
                    name,
                    public_id,
                    system_id,
                } => {
                    if name.len() > MAX_ATTRIBUTE_BYTES
                        || public_id
                            .as_ref()
                            .is_some_and(|value| value.len() > MAX_ATTRIBUTE_BYTES)
                        || system_id
                            .as_ref()
                            .is_some_and(|value| value.len() > MAX_ATTRIBUTE_BYTES)
                    {
                        return Err(NativeEngineError::limit(
                            "content-process document type",
                            MAX_ATTRIBUTE_BYTES,
                            MAX_ATTRIBUTE_BYTES.saturating_add(1),
                        ));
                    }
                    NativeNodeKind::DocumentType {
                        name: name.clone(),
                        public_id: public_id.clone(),
                        system_id: system_id.clone(),
                    }
                }
                NativeNodeKindWire::Comment(value) => {
                    if value.len() > MAX_ATTRIBUTE_BYTES {
                        return Err(NativeEngineError::limit(
                            "content-process comment",
                            MAX_ATTRIBUTE_BYTES,
                            value.len(),
                        ));
                    }
                    NativeNodeKind::Comment(value.clone())
                }
                NativeNodeKindWire::Text(value) => NativeNodeKind::Text(value.clone()),
            };
            let namespace_uri = if matches!(&kind, NativeNodeKind::Element { .. }) {
                match wire_node.state.namespace_uri.as_deref() {
                    Some("") => None,
                    Some(value) => Some(validate_namespace_uri(value)?),
                    None => Some(HTML_NAMESPACE_URI.to_owned()),
                }
            } else {
                None
            };
            if wire_node.state.form_associated_disabled.is_some()
                && !matches!(
                    (&kind, namespace_uri.as_deref()),
                    (
                        NativeNodeKind::Element { name, .. },
                        Some(HTML_NAMESPACE_URI)
                    ) if name.contains('-')
                )
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned form-disabled state for a non-HTML or non-hyphenated element".into(),
                });
            }
            let attribute_namespaces = if matches!(&kind, NativeNodeKind::Element { .. }) {
                wire_node
                    .state
                    .attribute_namespaces
                    .iter()
                    .map(|(name, namespace)| {
                        Ok((name.clone(), validate_attribute_namespace_uri(namespace)?))
                    })
                    .collect::<Result<BTreeMap<_, _>, NativeEngineError>>()?
            } else {
                BTreeMap::new()
            };
            if !wire_node.state.files.is_empty() {
                NativeFile::validate_many(&wire_node.state.files)?;
            }
            nodes.push(NativeNode {
                id: NativeNodeId { generation, index },
                parent,
                children,
                template_content,
                template_host,
                shadow_host: wire_node
                    .shadow_host
                    .map(|index| NativeNodeId { generation, index }),
                shadow_root: wire_node
                    .shadow_root
                    .map(|index| NativeNodeId { generation, index }),
                shadow_mode: wire_node.shadow_mode.clone(),
                kind,
                state: NativeElementState {
                    namespace_uri,
                    attribute_namespaces,
                    value: wire_node.state.value.clone(),
                    files: wire_node.state.files.clone(),
                    checked: wire_node.state.checked,
                    indeterminate: wire_node.state.indeterminate,
                    focused: wire_node.state.focused,
                    focus_anchor_node_index: wire_node.state.focus_anchor_node_index,
                    hovered: wire_node.state.hovered,
                    user_interacted: wire_node.state.user_interacted,
                    selected: wire_node.state.selected,
                    form_associated_disabled: wire_node.state.form_associated_disabled,
                    custom_validity: wire_node.state.custom_validity.clone(),
                    selection_start: wire_node.state.selection_start,
                    selection_end: wire_node.state.selection_end,
                    selection_direction: wire_node.state.selection_direction.clone(),
                    inline_style_allowed: !blocked_inline_style_nodes.contains(&index),
                },
            });
        }
        if !matches!(nodes[0].kind, NativeNodeKind::Document) || nodes[0].parent.is_some() {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an invalid document root".into(),
            });
        }
        for node in &nodes {
            match node.kind() {
                NativeNodeKind::Element { name, .. }
                    if name == "template" && node.namespace_uri() == Some(HTML_NAMESPACE_URI) =>
                {
                    let fragment_id =
                        node.template_content
                            .ok_or_else(|| NativeEngineError::Parse {
                                offset: 0,
                                reason: "content process returned a template without contents"
                                    .into(),
                            })?;
                    let fragment = nodes.get(fragment_id.index as usize).ok_or_else(|| {
                        NativeEngineError::Parse {
                            offset: 0,
                            reason: "content process returned an invalid template fragment".into(),
                        }
                    })?;
                    if !matches!(fragment.kind(), NativeNodeKind::DocumentFragment)
                        || fragment.parent().is_some()
                        || fragment.template_host != Some(node.id())
                        || node.children().contains(&fragment_id)
                    {
                        return Err(NativeEngineError::Parse {
                            offset: 0,
                            reason: "content process returned an inconsistent template fragment"
                                .into(),
                        });
                    }
                }
                NativeNodeKind::DocumentFragment => {
                    if let Some(host_id) = node.template_host {
                        let host = nodes.get(host_id.index as usize).ok_or_else(|| {
                            NativeEngineError::Parse {
                                offset: 0,
                                reason: "content process returned an invalid fragment owner".into(),
                            }
                        })?;
                        if node.parent().is_some() || host.template_content != Some(node.id()) {
                            return Err(NativeEngineError::Parse {
                                offset: 0,
                                reason: "content process returned an inconsistent fragment owner"
                                    .into(),
                            });
                        }
                    } else if node.parent().is_some() || node.template_content.is_some() {
                        return Err(NativeEngineError::Parse {
                            offset: 0,
                            reason: "content process returned an invalid detached fragment".into(),
                        });
                    }
                }
                _ if node.template_content.is_some() || node.template_host.is_some() => {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned invalid template links".into(),
                    });
                }
                _ => {}
            }
        }
        for node in &nodes {
            if let Some(root_id) = node.shadow_root {
                let root =
                    nodes
                        .get(root_id.index as usize)
                        .ok_or_else(|| NativeEngineError::Parse {
                            offset: 0,
                            reason: "content process returned an invalid shadow root".into(),
                        })?;
                if !matches!(node.kind(), NativeNodeKind::Element { .. })
                    || node
                        .shadow_mode
                        .as_deref()
                        .is_none_or(|mode| mode != "open" && mode != "closed")
                    || root.shadow_host != Some(node.id())
                    || root.shadow_mode != node.shadow_mode
                    || !matches!(root.kind(), NativeNodeKind::DocumentFragment)
                    || root.parent().is_some()
                    || root.template_host.is_some()
                {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned inconsistent shadow-root links".into(),
                    });
                }
            }
            if let Some(host_id) = node.shadow_host {
                let host =
                    nodes
                        .get(host_id.index as usize)
                        .ok_or_else(|| NativeEngineError::Parse {
                            offset: 0,
                            reason: "content process returned an invalid shadow host".into(),
                        })?;
                if !matches!(node.kind(), NativeNodeKind::DocumentFragment)
                    || node.parent().is_some()
                    || node.template_host.is_some()
                    || host.shadow_root != Some(node.id())
                    || node
                        .shadow_mode
                        .as_deref()
                        .is_none_or(|mode| mode != "open" && mode != "closed")
                {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned inconsistent shadow-host links".into(),
                    });
                }
            }
            if node.shadow_mode.is_some()
                && node.shadow_host.is_none()
                && node.shadow_root.is_none()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an orphan shadow mode".into(),
                });
            }
        }
        if wire.image_resources.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process image resources",
                limits.max_nodes,
                wire.image_resources.len(),
            ));
        }
        let max_encoded_pixels = (MAX_NATIVE_IMAGE_TRANSFER_BYTES
            .saturating_add(2)
            .saturating_div(3))
        .saturating_mul(4);
        let mut image_resources = BTreeMap::new();
        for resource in wire.image_resources {
            if resource.source.is_empty()
                || resource.source.len() > MAX_ATTRIBUTE_BYTES
                || resource.source.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid image source".into(),
                });
            }
            let encoded_pixels = resource
                .pixels_base64
                .len()
                .max(resource.pixels_zlib_base64.len());
            if encoded_pixels > max_encoded_pixels {
                return Err(NativeEngineError::limit(
                    "content-process image pixels",
                    max_encoded_pixels,
                    encoded_pixels,
                ));
            }
            let node_index =
                usize::try_from(resource.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid image node index".into(),
                })?;
            let _node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range image node index".into(),
                })?;
            if !is_image_resource_node_in(&nodes, node_index)
                || !image_source_is_declared(&nodes, node_index, &resource.source)
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an image resource for a different node"
                        .into(),
                });
            }
            let image = image_from_wire(&resource, max_encoded_pixels)?;
            if image_resources
                .insert(
                    resource.node_index,
                    NativeImageResource {
                        source: resource.source,
                        image,
                    },
                )
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate image resources".into(),
                });
            }
        }
        if wire.video_poster_resources.len() > MAX_NATIVE_VIDEO_POSTERS {
            return Err(NativeEngineError::limit(
                "content-process video poster resources",
                MAX_NATIVE_VIDEO_POSTERS,
                wire.video_poster_resources.len(),
            ));
        }
        let mut video_poster_resources = BTreeMap::new();
        for resource in wire.video_poster_resources {
            if resource.source.is_empty()
                || resource.source.len() > MAX_ATTRIBUTE_BYTES
                || resource.source.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid video poster source".into(),
                });
            }
            if resource.pixels_base64.len() > max_encoded_pixels {
                return Err(NativeEngineError::limit(
                    "content-process video poster pixels",
                    max_encoded_pixels,
                    resource.pixels_base64.len(),
                ));
            }
            let node_index =
                usize::try_from(resource.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid video poster node index".into(),
                })?;
            let _node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range video poster node index"
                        .into(),
                })?;
            if !video_poster_source_is_declared(&nodes, node_index, &resource.source) {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned a poster resource for a different video"
                        .into(),
                });
            }
            let image = image_from_wire(&resource, max_encoded_pixels)?;
            if video_poster_resources
                .insert(
                    resource.node_index,
                    NativeImageResource {
                        source: resource.source,
                        image,
                    },
                )
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate video poster resources".into(),
                });
            }
        }
        if wire.background_image_resources.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process background image resources",
                limits.max_nodes,
                wire.background_image_resources.len(),
            ));
        }
        let mut background_image_resources = BTreeMap::new();
        for resource in wire.background_image_resources {
            if resource.source.is_empty()
                || resource.source.len() > MAX_ATTRIBUTE_BYTES
                || resource.source.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid background image resource".into(),
                });
            }
            if resource.pixels_base64.len() > max_encoded_pixels {
                return Err(NativeEngineError::limit(
                    "content-process background image pixels",
                    max_encoded_pixels,
                    resource.pixels_base64.len(),
                ));
            }
            let node_index =
                usize::try_from(resource.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid background image node index"
                        .into(),
                })?;
            let node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range background image node index"
                        .into(),
                })?;
            let style = wire
                .computed_styles
                .get(node_index)
                .copied()
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range background image style"
                        .into(),
                })?;
            let source_id = style
                .background_image()
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned a background image for a node without one"
                        .into(),
                })?;
            if background_image_sources.get(&source_id) != Some(&resource.source)
                || !matches!(node.kind(), NativeNodeKind::Element { .. })
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason:
                        "content process returned a background image resource for a different style"
                            .into(),
                });
            }
            let image = image_from_wire(&resource, max_encoded_pixels)?;
            if background_image_resources
                .insert(
                    resource.node_index,
                    NativeImageResource {
                        source: resource.source,
                        image,
                    },
                )
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate background image resources".into(),
                });
            }
        }
        if wire.image_loads.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process image load states",
                limits.max_nodes,
                wire.image_loads.len(),
            ));
        }
        let mut image_loads = BTreeMap::new();
        for load in wire.image_loads {
            if load.source.is_empty()
                || load.source.len() > MAX_ATTRIBUTE_BYTES
                || load.source.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid image load source".into(),
                });
            }
            let node_index =
                usize::try_from(load.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid image load node index".into(),
                })?;
            let _node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range image load node index".into(),
                })?;
            if !is_image_resource_node_in(&nodes, node_index)
                || !image_source_is_declared(&nodes, node_index, &load.source)
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an image load for a different node".into(),
                });
            }
            if image_loads.insert(load.node_index, load.source).is_some() {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate image load states".into(),
                });
            }
        }
        if wire.video_poster_loads.len() > MAX_NATIVE_VIDEO_POSTERS {
            return Err(NativeEngineError::limit(
                "content-process video poster load states",
                MAX_NATIVE_VIDEO_POSTERS,
                wire.video_poster_loads.len(),
            ));
        }
        let mut video_poster_loads = BTreeMap::new();
        for load in wire.video_poster_loads {
            if load.source.is_empty()
                || load.source.len() > MAX_ATTRIBUTE_BYTES
                || load.source.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid video poster load source".into(),
                });
            }
            let node_index =
                usize::try_from(load.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid video poster load node index"
                        .into(),
                })?;
            if !video_poster_source_is_declared(&nodes, node_index, &load.source) {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned a video poster load for a different node"
                        .into(),
                });
            }
            if video_poster_loads
                .insert(load.node_index, load.source)
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate video poster load states".into(),
                });
            }
        }
        if video_poster_resources.iter().any(|(node_index, resource)| {
            video_poster_loads.get(node_index) != Some(&resource.source)
        }) {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned video poster resources without matching loads"
                    .into(),
            });
        }
        if wire.media_resources.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process media resources",
                limits.max_nodes,
                wire.media_resources.len(),
            ));
        }
        let mut media_resources = BTreeMap::new();
        let mut document_video_bytes = 0usize;
        let mut document_video_frame_count = 0usize;
        let mut document_audio_bytes = 0usize;
        for resource in wire.media_resources {
            if resource.source.is_empty()
                || resource.source.len() > MAX_ATTRIBUTE_BYTES
                || resource.source.bytes().any(|byte| byte.is_ascii_control())
                || resource.content_type.is_empty()
                || resource.content_type.len() > MAX_ATTRIBUTE_BYTES
                || resource.byte_length == 0
                || resource.byte_length > MAX_NATIVE_MEDIA_BYTES
                || !media_type_is_supported(Some(&resource.content_type))
                || resource.playback_rate_millionths == 0
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid media resource".into(),
                });
            }
            let node_index =
                usize::try_from(resource.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid media node index".into(),
                })?;
            let node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range media node index".into(),
                })?;
            if (node.element_name() != Some("audio") && node.element_name() != Some("video"))
                || !media_source_is_declared(&nodes, node_index, &resource.source)
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned a media resource for a different node".into(),
                });
            }
            let mut decoded_video_bytes = 0usize;
            let valid_video_frames = resource.video_frames.len() <= MAX_NATIVE_VIDEO_FRAME_COUNT
                && resource.video_frames.iter().all(|frame| {
                    let expected = (frame.width as usize)
                        .checked_mul(frame.height as usize)
                        .and_then(|pixels| pixels.checked_mul(4));
                    let Some(expected) = expected else {
                        return false;
                    };
                    decoded_video_bytes = decoded_video_bytes.saturating_add(expected);
                    frame.width > 0
                        && frame.height > 0
                        && frame.width <= MAX_NATIVE_VIDEO_DIMENSION
                        && frame.height <= MAX_NATIVE_VIDEO_DIMENSION
                        && frame.pixels.len() == expected
                        && decoded_video_bytes <= MAX_NATIVE_VIDEO_DECODED_BYTES
                });
            document_video_bytes = document_video_bytes.saturating_add(decoded_video_bytes);
            document_video_frame_count =
                document_video_frame_count.saturating_add(resource.video_frames.len());
            if !valid_video_frames
                || (!resource.video_frames.is_empty() && node.element_name() != Some("video"))
                || (resource.is_playing && resource.video_frames.is_empty())
                || document_video_bytes > MAX_NATIVE_DOCUMENT_VIDEO_BYTES
                || document_video_frame_count > MAX_NATIVE_DOCUMENT_VIDEO_FRAMES
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned invalid or over-budget video frames".into(),
                });
            }
            let audio_track = if let Some(track) = resource.audio_track {
                let pcm_s16le = base64::engine::general_purpose::STANDARD
                    .decode(track.pcm_s16le_base64.as_bytes())
                    .map_err(|_| NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned malformed audio samples".into(),
                    })?;
                let valid = (track.channels == 1 || track.channels == 2)
                    && (8_000..=96_000).contains(&track.sample_rate)
                    && !pcm_s16le.is_empty()
                    && pcm_s16le
                        .len()
                        .is_multiple_of(usize::from(track.channels) * 2)
                    && pcm_s16le.len() <= MAX_NATIVE_AUDIO_DECODED_BYTES
                    && resource.content_type == "audio/wav"
                    && resource.duration_millis
                        == Some(
                            (pcm_s16le.len() as u64 / (u64::from(track.channels) * 2))
                                .saturating_mul(1000)
                                / u64::from(track.sample_rate),
                        )
                    && node.element_name() == Some("audio");
                document_audio_bytes = document_audio_bytes.saturating_add(pcm_s16le.len());
                if !valid || document_audio_bytes > MAX_NATIVE_DOCUMENT_AUDIO_DECODED_BYTES {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned invalid or over-budget audio samples"
                            .into(),
                    });
                }
                Some(NativeAudioTrack {
                    channels: track.channels,
                    sample_rate: track.sample_rate,
                    pcm_s16le,
                })
            } else {
                None
            };
            if media_resources
                .insert(
                    resource.node_index,
                    NativeMediaResource {
                        source: resource.source,
                        content_type: resource.content_type,
                        byte_length: resource.byte_length,
                        duration_millis: resource.duration_millis,
                        video_frames: resource.video_frames,
                        audio_track,
                        current_time_millis: resource.current_time_millis,
                        is_playing: resource.is_playing,
                        playback_rate_millionths: resource.playback_rate_millionths,
                        playback_started_at: resource.is_playing.then(std::time::Instant::now),
                    },
                )
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate media resources".into(),
                });
            }
        }
        let decode_media_loads = |loads: Vec<NativeMediaLoadWire>,
                                  kind: &'static str|
         -> Result<BTreeMap<u32, String>, NativeEngineError> {
            if loads.len() > limits.max_nodes {
                return Err(NativeEngineError::limit(
                    kind,
                    limits.max_nodes,
                    loads.len(),
                ));
            }
            let mut result = BTreeMap::new();
            for load in loads {
                if load.source.is_empty()
                    || load.source.len() > MAX_ATTRIBUTE_BYTES
                    || load.source.bytes().any(|byte| byte.is_ascii_control())
                {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned an invalid media load source".into(),
                    });
                }
                let node_index =
                    usize::try_from(load.node_index).map_err(|_| NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned an invalid media load node index".into(),
                    })?;
                let node = nodes
                    .get(node_index)
                    .ok_or_else(|| NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned an out-of-range media load node index"
                            .into(),
                    })?;
                if (node.element_name() != Some("audio") && node.element_name() != Some("video"))
                    || !media_source_is_declared(&nodes, node_index, &load.source)
                {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned a media load for a different node".into(),
                    });
                }
                if result.insert(load.node_index, load.source).is_some() {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned duplicate media load states".into(),
                    });
                }
            }
            Ok(result)
        };
        let media_loads =
            decode_media_loads(wire.media_loads, "content-process media load states")?;
        let media_errors =
            decode_media_loads(wire.media_errors, "content-process media error states")?;
        if media_resources
            .iter()
            .any(|(node_index, resource)| media_loads.get(node_index) != Some(&resource.source))
        {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned media resources without matching loads".into(),
            });
        }
        if wire.canvas_resources.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process canvas resources",
                limits.max_nodes,
                wire.canvas_resources.len(),
            ));
        }
        let max_canvas_encoded_bytes =
            (MAX_NATIVE_CANVAS_BYTES.saturating_add(2).saturating_div(3)).saturating_mul(4);
        let mut canvas_resources = BTreeMap::new();
        for resource in wire.canvas_resources {
            if resource.width == 0
                || resource.height == 0
                || resource.width > MAX_NATIVE_CANVAS_DIMENSION
                || resource.height > MAX_NATIVE_CANVAS_DIMENSION
            {
                return Err(NativeEngineError::invalid(
                    "content-process canvas dimensions",
                    format!("must be between 1 and {MAX_NATIVE_CANVAS_DIMENSION} pixels per axis"),
                ));
            }
            if resource.pixels_base64.len() > max_canvas_encoded_bytes {
                return Err(NativeEngineError::limit(
                    "content-process canvas pixels",
                    max_canvas_encoded_bytes,
                    resource.pixels_base64.len(),
                ));
            }
            let expected_bytes = usize::try_from(resource.width)
                .ok()
                .and_then(|width| {
                    usize::try_from(resource.height)
                        .ok()
                        .and_then(|height| width.checked_mul(height))
                })
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or_else(|| {
                    NativeEngineError::limit(
                        "content-process canvas pixels",
                        MAX_NATIVE_CANVAS_BYTES,
                        usize::MAX,
                    )
                })?;
            if expected_bytes > MAX_NATIVE_CANVAS_BYTES {
                return Err(NativeEngineError::limit(
                    "content-process canvas pixels",
                    MAX_NATIVE_CANVAS_BYTES,
                    expected_bytes,
                ));
            }
            let node_index =
                usize::try_from(resource.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid canvas node index".into(),
                })?;
            let node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range canvas node index".into(),
                })?;
            if node.element_name() != Some("canvas") {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned a canvas resource for a different node"
                        .into(),
                });
            }
            let pixels = base64::engine::general_purpose::STANDARD
                .decode(&resource.pixels_base64)
                .map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned invalid canvas pixels".into(),
                })?;
            if pixels.len() != expected_bytes {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned canvas pixels with the wrong dimensions"
                        .into(),
                });
            }
            if canvas_resources
                .insert(
                    resource.node_index,
                    NativeCanvasResource {
                        width: resource.width,
                        height: resource.height,
                        pixels,
                        origin_clean: resource.origin_clean,
                    },
                )
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate canvas resources".into(),
                });
            }
        }
        let root = NativeNodeId {
            generation,
            index: 0,
        };
        let mut diagnostics = NativeDiagnosticSink::default();
        for node in &nodes {
            if !node.inline_style_allowed() {
                continue;
            }
            let Some(inline_style) = node.attribute("style") else {
                continue;
            };
            css::collect_declaration_diagnostics(
                inline_style,
                NativeDiagnosticSource::InlineStyle {
                    node_index: node.id().index(),
                },
                0,
                &mut diagnostics,
            );
        }
        let (diagnostics, diagnostics_truncated) = diagnostics.finish();
        let mut script_node_ids = BTreeMap::new();
        for identity in wire.script_nodes {
            if identity.temporary_index < SCRIPT_TEMP_NODE_BASE {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid script node identity".into(),
                });
            }
            let index =
                usize::try_from(identity.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid script node identity".into(),
                })?;
            if index >= nodes.len()
                || script_node_ids
                    .insert(
                        identity.temporary_index,
                        NativeNodeId {
                            generation,
                            index: identity.node_index,
                        },
                    )
                    .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate script node identity".into(),
                });
            }
        }
        let mut started_script_nodes = BTreeSet::new();
        for node_index in wire.started_script_nodes {
            let index = usize::try_from(node_index).map_err(|_| NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an invalid started script node".into(),
            })?;
            if nodes.get(index).and_then(NativeNode::element_name) != Some("script")
                || !started_script_nodes.insert(node_index)
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid started script node".into(),
                });
            }
        }
        if embedded_frame_owner_removal_sequences
            .iter()
            .any(|(node_index, sequence)| {
                usize::try_from(*node_index).map_or(true, |index| {
                    index >= nodes.len()
                        || !matches!(
                            nodes.get(index).and_then(NativeNode::element_name),
                            Some("iframe" | "frame")
                        )
                }) || *sequence == 0
                    || *sequence > next_embedded_frame_owner_removal_sequence
            })
        {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned invalid iframe-removal bookkeeping".into(),
            });
        }
        let mut document = Self {
            generation,
            revision: u64::from(generation),
            root,
            max_nodes: limits.max_nodes,
            max_dom_depth: limits.max_dom_depth,
            quirks_mode,
            document_referrer_policy,
            inherited_base_url,
            nodes,
            stylesheet: NativeStylesheet::default(),
            font_resources: font_resources.clone(),
            font_book,
            external_stylesheet_states: BTreeMap::new(),
            external_image_preload_states: BTreeMap::new(),
            preloaded_image_resources: BTreeMap::new(),
            viewport,
            computed_styles: Some(wire.computed_styles),
            layout_style_cache: NativeLayoutStyleCache::default(),
            css_target_fragment: None,
            diagnostics,
            diagnostics_truncated,
            script_node_ids,
            started_script_nodes,
            processed_csp_meta_nodes: BTreeSet::new(),
            pending_csp_meta_policies: BTreeMap::new(),
            embedded_frame_owner_removal_sequences,
            next_embedded_frame_owner_removal_sequence,
            image_resources,
            image_loads,
            video_poster_resources,
            video_poster_loads,
            media_resources,
            media_loads,
            media_errors,
            background_image_sources,
            background_image_resources,
            canvas_resources,
            inline_style_element_reports: BTreeMap::new(),
            inline_style_attribute_reports: BTreeMap::new(),
        };
        for node in &document.nodes {
            let Some(anchor_index) = node.state.focus_anchor_node_index else {
                continue;
            };
            let anchor = document
                .nodes
                .get(
                    usize::try_from(anchor_index).map_err(|_| NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned an invalid focus anchor index".into(),
                    })?,
                )
                .map(NativeNode::id)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range focus anchor".into(),
                })?;
            if !node.state.focused
                || node.element_name() != Some("area")
                || !document
                    .image_map_area_focus_anchors(node.id())
                    .contains(&anchor)
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid image-map focus anchor".into(),
                });
            }
        }
        document.normalize_select_defaults();
        document.mark_inline_style_reports_seen();
        document.mark_content_security_policy_meta_processed();
        Ok(document)
    }
}

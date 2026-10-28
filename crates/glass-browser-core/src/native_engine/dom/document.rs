//! Native document state, mutations, traversal, and semantic projections.

use super::*;
use sha2::{Digest, Sha256};

impl NativeDocument {
    pub(crate) fn has_inline_event_handler(&self, event_type: &str) -> bool {
        let attribute_name = format!("on{}", event_type.to_ascii_lowercase());
        self.nodes
            .iter()
            .any(|node| node.attribute(&attribute_name).is_some())
    }

    pub(crate) fn set_viewport(&mut self, viewport: Viewport) -> Result<(), NativeEngineError> {
        viewport.validate()?;
        if self.viewport != viewport {
            self.viewport = viewport;
            self.invalidate_layout_styles();
        }
        Ok(())
    }

    pub(crate) fn set_css_target_fragment(&mut self, fragment: Option<String>) {
        if self.css_target_fragment != fragment {
            self.css_target_fragment = fragment;
            self.invalidate_layout_styles();
        }
    }

    pub(crate) fn set_css_target_from_url(
        &mut self,
        document_url: &str,
    ) -> Result<(), NativeEngineError> {
        let parsed = Url::parse(document_url).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "document URL is not valid URL syntax".into(),
        })?;
        let fragment = parsed.fragment().and_then(decode_percent_encoded_fragment);
        self.set_css_target_fragment(fragment);
        Ok(())
    }

    pub(crate) fn css_target(&self, id: NativeNodeId) -> bool {
        self.css_target_fragment
            .as_deref()
            .and_then(|fragment| self.fragment_target(fragment))
            == Some(id)
    }

    pub(crate) fn inline_style_elements(&self) -> Vec<(u32, String, Option<String>)> {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()) && node.element_name() == Some("style"))
            .map(|node| {
                let mut source = String::new();
                self.collect_raw_text(node.id(), &mut source);
                (
                    node.id().index(),
                    source,
                    node.attribute("nonce").map(str::to_owned),
                )
            })
            .collect()
    }

    pub(crate) fn inline_style_attributes(&self) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()) && node.attribute("style").is_some())
            .filter_map(|node| {
                node.attribute("style")
                    .map(|source| (node.id().index(), source.to_owned()))
            })
            .collect()
    }

    /// Mark the currently attached inline styles as already evaluated by the
    /// document loader. The initial parser probes styles once to determine
    /// enforced policy; the committed document must not report those same
    /// declarations again during its first refresh.
    pub(crate) fn mark_inline_style_reports_seen(&mut self) {
        self.inline_style_element_reports = self
            .inline_style_elements()
            .into_iter()
            .map(|(node_index, source, nonce)| (node_index, (source, nonce)))
            .collect();
        self.inline_style_attribute_reports = self.inline_style_attributes().into_iter().collect();
    }

    pub(crate) fn inline_style_element_reported(
        &self,
        node_index: u32,
        source: &str,
        nonce: Option<&str>,
    ) -> bool {
        self.inline_style_element_reports
            .get(&node_index)
            .is_some_and(|(reported_source, reported_nonce)| {
                reported_source == source && reported_nonce.as_deref() == nonce
            })
    }

    pub(crate) fn mark_inline_style_element_reported(
        &mut self,
        node_index: u32,
        source: String,
        nonce: Option<String>,
    ) {
        self.inline_style_element_reports
            .insert(node_index, (source, nonce));
    }

    pub(crate) fn inline_style_attribute_reported(&self, node_index: u32, source: &str) -> bool {
        self.inline_style_attribute_reports
            .get(&node_index)
            .is_some_and(|reported_source| reported_source == source)
    }

    pub(crate) fn mark_inline_style_attribute_reported(&mut self, node_index: u32, source: String) {
        self.inline_style_attribute_reports
            .insert(node_index, source);
    }

    pub(crate) fn prune_inline_style_reports(
        &mut self,
        element_nodes: &BTreeSet<u32>,
        attribute_nodes: &BTreeSet<u32>,
    ) {
        self.inline_style_element_reports
            .retain(|node_index, _| element_nodes.contains(node_index));
        self.inline_style_attribute_reports
            .retain(|node_index, _| attribute_nodes.contains(node_index));
    }

    /// Return parser-time enforced CSP policies declared by `meta` elements
    /// in the document head. Report-only values are intentionally excluded;
    /// the loader owns enforcement and reporting is a separate contract.
    /// Returning one extra value lets the loader reject an over-limit document
    /// instead of silently weakening its policy surface.
    pub(crate) fn content_security_policy_meta(&self) -> Vec<String> {
        self.content_security_policy_meta_entries()
            .into_iter()
            .take(MAX_NATIVE_CSP_POLICIES.saturating_add(1))
            .map(|(_, value)| value)
            .collect()
    }

    pub(super) fn content_security_policy_meta_entries(&self) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter(|node| {
                self.is_attached(node.id())
                    && node.element_name() == Some("meta")
                    && self.is_descendant_of_element(node.id(), "head")
                    && node.attribute("http-equiv").is_some_and(|value| {
                        value.trim().eq_ignore_ascii_case("content-security-policy")
                    })
            })
            .filter_map(|node| {
                node.attribute("content")
                    .filter(|value| !value.is_empty())
                    .map(|value| (node.id().index(), value.to_owned()))
            })
            .collect()
    }

    /// Mark the parser-discovered CSP meta elements as processed by the
    /// document policy container. Meta policies are additive and remain
    /// active after their element is removed; this ledger also makes later
    /// content attribute edits inert as required by CSP.
    pub(crate) fn mark_content_security_policy_meta_processed(&mut self) {
        self.processed_csp_meta_nodes.extend(
            self.content_security_policy_meta_entries()
                .into_iter()
                .map(|(node_index, _)| node_index),
        );
    }

    pub(crate) fn processed_content_security_policy_meta_nodes(&self) -> Vec<u32> {
        let mut processed = self.processed_csp_meta_nodes.clone();
        for (script_index, node_id) in &self.script_node_ids {
            if self.processed_csp_meta_nodes.contains(&node_id.index()) {
                processed.insert(*script_index);
            }
        }
        processed.into_iter().collect()
    }

    /// Return head meta policies that have not yet been handed to the policy
    /// container. Policies captured at insertion remain pending even if the
    /// node is later detached or edited. The caller commits the node indexes
    /// only after the loader accepts the append, preserving atomic failure.
    pub(crate) fn unprocessed_content_security_policy_meta(&self) -> Vec<(u32, String)> {
        let mut entries = self
            .pending_csp_meta_policies
            .iter()
            .map(|(node_index, value)| (*node_index, value.clone()))
            .collect::<Vec<_>>();
        entries.extend(
            self.content_security_policy_meta_entries()
                .into_iter()
                .filter(|(node_index, _)| {
                    !self.processed_csp_meta_nodes.contains(node_index)
                        && !self.pending_csp_meta_policies.contains_key(node_index)
                }),
        );
        entries.sort_by_key(|(node_index, _)| *node_index);
        entries
            .into_iter()
            .take(MAX_NATIVE_CSP_POLICIES.saturating_add(1))
            .collect()
    }

    pub(crate) fn mark_content_security_policy_meta_nodes_processed(
        &mut self,
        node_indexes: impl IntoIterator<Item = u32>,
    ) {
        for node_index in node_indexes {
            self.processed_csp_meta_nodes.insert(node_index);
            self.pending_csp_meta_policies.remove(&node_index);
        }
    }

    pub(super) fn capture_attached_content_security_policy_meta(&mut self) {
        for (node_index, policy) in self.content_security_policy_meta_entries() {
            if !self.processed_csp_meta_nodes.contains(&node_index) {
                self.pending_csp_meta_policies
                    .entry(node_index)
                    .or_insert(policy);
            }
        }
    }

    pub(crate) fn set_inline_style_policy(&mut self, allowed_nodes: &BTreeSet<u32>) {
        let mut changed = false;
        for node in &mut self.nodes {
            if node.element_name() == Some("style") || node.attribute("style").is_some() {
                let allowed = allowed_nodes.contains(&node.id().index());
                if node.state.inline_style_allowed != allowed {
                    node.state.inline_style_allowed = allowed;
                    changed = true;
                }
            }
        }
        if changed {
            self.invalidate_layout_styles();
        }
    }

    pub(crate) fn set_external_stylesheet_states(
        &mut self,
        states: impl IntoIterator<Item = (u32, String, Option<String>, Option<String>)>,
    ) {
        self.external_stylesheet_states = states
            .into_iter()
            .map(|(node_index, href, stylesheet_url, body)| {
                (
                    node_index,
                    NativeExternalStylesheetState {
                        href,
                        stylesheet_url,
                        body,
                    },
                )
            })
            .collect();
    }

    pub(crate) fn set_external_image_preload_states(
        &mut self,
        states: impl IntoIterator<Item = (u32, String)>,
    ) {
        self.external_image_preload_states = states.into_iter().collect();
    }

    pub(crate) fn font_face_rules(&self) -> &[NativeFontFaceRule] {
        self.stylesheet.font_face_rules()
    }

    pub(crate) fn font_face_script_descriptors(&self) -> Vec<NativeFontFaceScriptDescriptor> {
        self.stylesheet
            .font_face_rules()
            .iter()
            .map(|rule| NativeFontFaceScriptDescriptor {
                family: rule.family.clone(),
                weight: format_font_weight_range(rule.weight),
                style: match rule.style {
                    FontStyleValue::Normal => "normal".into(),
                    FontStyleValue::Italic => "italic".into(),
                },
                stretch: format_font_stretch_range(rule.stretch),
                unicode_range: format_font_face_unicode_ranges(&rule.unicode_ranges),
                feature_settings: format_font_feature_settings(rule.feature_settings),
                variation_settings: format_font_variation_settings(rule.variation_settings),
                size_adjust: format_font_size_adjust(rule.size_adjust),
                ascent_override: format_font_metric_override(rule.metric_overrides.ascent),
                descent_override: format_font_metric_override(rule.metric_overrides.descent),
                line_gap_override: format_font_metric_override(rule.metric_overrides.line_gap),
                display: rule.font_display.as_str().into(),
                status: if self.font_resources.iter().any(|resource| {
                    resource.family_key == rule.family_key
                        && resource.weight == rule.weight
                        && resource.style == rule.style
                        && resource.variation_settings == rule.variation_settings
                        && resource.feature_settings == rule.feature_settings
                        && resource.size_adjust == rule.size_adjust
                        && resource.font_display == rule.font_display
                        && resource.metric_overrides == rule.metric_overrides
                }) {
                    "loaded".into()
                } else {
                    "error".into()
                },
            })
            .collect()
    }

    pub(crate) fn set_font_resources(
        &mut self,
        resources: Vec<NativeFontFaceResource>,
    ) -> Result<(), NativeEngineError> {
        if resources.len() > MAX_NATIVE_FONT_FACES {
            return Err(NativeEngineError::limit(
                "native font faces",
                MAX_NATIVE_FONT_FACES,
                resources.len(),
            ));
        }
        let total_bytes = resources.iter().try_fold(0usize, |total, resource| {
            if resource.family.is_empty() || resource.family.len() > MAX_ATTRIBUTE_BYTES {
                return None;
            }
            if resource.weight.min < 1
                || resource.weight.min > resource.weight.max
                || resource.weight.max > 1000
            {
                return None;
            }
            if resource.stretch.min < 500
                || resource.stretch.min > resource.stretch.max
                || resource.stretch.max > 2000
            {
                return None;
            }
            if resource.bytes.is_empty() || resource.bytes.len() > MAX_NATIVE_FONT_BYTES {
                return None;
            }
            if !resource.variation_settings.is_valid() {
                return None;
            }
            total.checked_add(resource.bytes.len())
        });
        let Some(total_bytes) = total_bytes else {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "native font resource metadata is invalid".into(),
            });
        };
        if total_bytes > MAX_NATIVE_FONT_TOTAL_BYTES {
            return Err(NativeEngineError::limit(
                "native font resource bytes",
                MAX_NATIVE_FONT_TOTAL_BYTES,
                total_bytes,
            ));
        }
        if self.font_resources != resources {
            self.layout_style_cache.clear_text_widths();
            self.font_book = NativeFontBook::from_resources(&resources);
            self.font_resources = resources;
        }
        Ok(())
    }

    /// Install one script-created `FontFace` after its source bytes have
    /// already passed the page/network loader. Keeping the final admission in
    /// the document owner makes local and process-backed documents share the
    /// same bounds, descriptor normalization, and font-book rebuild.
    pub(super) fn apply_script_font_face_install(
        &mut self,
        command: &NativeScriptCommand,
    ) -> Result<(), NativeEngineError> {
        let NativeScriptCommand::FontFaceInstall {
            request_id,
            family,
            weight,
            style,
            stretch,
            unicode_range,
            variant,
            feature_settings,
            size_adjust,
            ascent_override,
            descent_override,
            line_gap_override,
            display,
            variation_settings,
            body_base64,
        } = command
        else {
            return Err(NativeEngineError::invalid(
                "native FontFace install",
                "command is not a FontFace installation",
            ));
        };
        if *request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native FontFace request id",
                "must be positive",
            ));
        }
        let family = family.trim();
        if family.is_empty()
            || family.len() > MAX_ATTRIBUTE_BYTES
            || family.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(NativeEngineError::invalid(
                "native FontFace family",
                "must be a bounded non-empty name",
            ));
        }
        let weight = parse_font_weight_range(weight).ok_or_else(|| {
            NativeEngineError::invalid(
                "native FontFace weight",
                "must be one absolute weight or an ascending 1-1000 range",
            )
        })?;
        let style = match style.trim().to_ascii_lowercase().as_str() {
            "normal" => FontStyleValue::Normal,
            "italic" => FontStyleValue::Italic,
            _ => {
                return Err(NativeEngineError::invalid(
                    "native FontFace style",
                    "must be normal or italic",
                ));
            }
        };
        let stretch = if stretch.trim().is_empty() {
            super::super::css::NativeFontStretchRange::default()
        } else {
            parse_font_stretch_range(stretch).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace stretch",
                    "must be a bounded CSS font-stretch value",
                )
            })?
        };
        let unicode_ranges = if unicode_range.trim().is_empty() {
            Vec::new()
        } else {
            parse_font_face_unicode_range(unicode_range).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace unicodeRange",
                    "must be a bounded CSS unicode range list",
                )
            })?
        };
        let variant_settings = if variant.trim().is_empty() {
            NativeFontFeatureSettings::default()
        } else {
            parse_font_face_variant(variant).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace variant",
                    "must be a bounded CSS font-variant value",
                )
            })?
        };
        let variation_settings = if variation_settings.trim().is_empty() {
            NativeFontVariationSettings::default()
        } else {
            parse_font_variation_settings(variation_settings).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace variationSettings",
                    "must be normal or a bounded CSS font-variation-settings value",
                )
            })?
        };
        let feature_settings = if feature_settings.trim().is_empty() {
            NativeFontFeatureSettings::default()
        } else {
            parse_font_feature_settings(feature_settings).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace featureSettings",
                    "must be normal or a bounded CSS font-feature-settings value",
                )
            })?
        };
        let feature_settings = variant_settings.with_overrides(feature_settings);
        let size_adjust = if size_adjust.trim().is_empty() {
            DEFAULT_NATIVE_FONT_SIZE_ADJUST
        } else {
            parse_font_size_adjust(size_adjust).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace sizeAdjust",
                    "must be a bounded CSS percentage from 25% through 400%",
                )
            })?
        };
        let parse_metric_override = |value: &str, field: &str| {
            if value.trim().is_empty() {
                Ok(None)
            } else {
                parse_font_metric_override(value).ok_or_else(|| {
                    NativeEngineError::invalid(
                        field,
                        "must be normal or a bounded CSS percentage from 0% through 1000%",
                    )
                })
            }
        };
        let metric_overrides = NativeFontMetricOverrides {
            ascent: parse_metric_override(ascent_override, "native FontFace ascentOverride")?,
            descent: parse_metric_override(descent_override, "native FontFace descentOverride")?,
            line_gap: parse_metric_override(line_gap_override, "native FontFace lineGapOverride")?,
        };
        let font_display = if display.trim().is_empty() {
            NativeFontDisplay::Auto
        } else {
            NativeFontDisplay::parse(display).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace display",
                    "must be one of auto, block, swap, fallback, or optional",
                )
            })?
        };
        let max_encoded_bytes = (MAX_NATIVE_FONT_BYTES.saturating_add(2) / 3).saturating_mul(4);
        if body_base64.len() > max_encoded_bytes {
            return Err(NativeEngineError::limit(
                "native FontFace bytes",
                max_encoded_bytes,
                body_base64.len(),
            ));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(body_base64)
            .map_err(|_| {
                NativeEngineError::invalid("native FontFace bytes", "must be valid base64")
            })?;
        if bytes.is_empty() || bytes.len() > MAX_NATIVE_FONT_BYTES {
            return Err(NativeEngineError::limit(
                "native FontFace bytes",
                MAX_NATIVE_FONT_BYTES,
                bytes.len(),
            ));
        }
        if !NativeFontBook::is_parseable_font_bytes(&bytes) {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "native FontFace bytes are not a supported font format".into(),
            });
        }
        let resource = NativeFontFaceResource {
            family: family.to_owned(),
            family_key: font_family_hash(family),
            weight,
            style,
            stretch,
            size_adjust,
            metric_overrides,
            font_display,
            bytes: Arc::from(bytes),
            unicode_ranges,
            variation_settings,
            feature_settings,
        };
        let mut resources = self.font_resources.clone();
        resources.push(resource);
        self.set_font_resources(resources)
    }

    pub(crate) fn apply_script_font_face_installs(
        &mut self,
        commands: &[NativeScriptCommand],
    ) -> Result<(), NativeEngineError> {
        for command in commands {
            if !matches!(command, NativeScriptCommand::FontFaceInstall { .. }) {
                continue;
            }
            self.apply_script_font_face_install(command)?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn assign_parsed_namespaces(&mut self) {
        let children = self
            .raw_node(self.root)
            .map(NativeNode::children)
            .unwrap_or_default()
            .to_vec();
        for child in children {
            self.assign_parsed_namespace_subtree(child, self.root);
        }
    }

    #[cfg(test)]
    pub(super) fn assign_parsed_namespace_to_node(
        &mut self,
        id: NativeNodeId,
        parent: NativeNodeId,
    ) -> Result<(), NativeEngineError> {
        self.assign_parsed_namespace_to_node_with_html_context(id, parent, false)
    }

    #[cfg(test)]
    pub(super) fn assign_parsed_namespace_to_node_with_html_context(
        &mut self,
        id: NativeNodeId,
        parent: NativeNodeId,
        force_html_parent_rules: bool,
    ) -> Result<(), NativeEngineError> {
        let parent_node = self
            .raw_node(parent)
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder referenced an unknown namespace parent".into(),
            })?;
        let name = self
            .raw_node(id)
            .and_then(NativeNode::element_name)
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder namespace assignment targeted a non-element".into(),
            })?;
        let namespace = if force_html_parent_rules {
            parsed_html_child_namespace(name)
        } else {
            parsed_child_namespace(parent_node, name)
        };
        let node = self
            .raw_node_mut(id)
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder lost an element during namespace assignment".into(),
            })?;
        node.state.namespace_uri = Some(namespace.to_owned());
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn adjust_parsed_foreign_attributes(
        &mut self,
        id: NativeNodeId,
    ) -> Result<(), NativeEngineError> {
        let node = self
            .raw_node_mut(id)
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder lost an element during attribute adjustment".into(),
            })?;
        adjust_parsed_attributes_for_namespace(node);
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn assign_parsed_namespace_subtree(
        &mut self,
        id: NativeNodeId,
        parent: NativeNodeId,
    ) {
        let Some(parent_node) = self.raw_node(parent) else {
            return;
        };
        let Some(node) = self.raw_node(id) else {
            return;
        };
        let Some(name) = node.element_name().map(str::to_owned) else {
            return;
        };
        let children = node.children().to_vec();
        let namespace = parsed_child_namespace(parent_node, &name);
        if let Some(node) = self.raw_node_mut(id) {
            node.state.namespace_uri = Some(namespace.to_owned());
        }
        for child in children {
            self.assign_parsed_namespace_subtree(child, id);
        }
    }

    pub(crate) fn empty() -> Self {
        let root = NativeNodeId {
            generation: 1,
            index: 0,
        };
        Self {
            generation: 1,
            revision: 0,
            root,
            max_nodes: MAX_NATIVE_NODES,
            max_dom_depth: MAX_NATIVE_DOM_DEPTH,
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
            computed_styles: None,
            layout_style_cache: NativeLayoutStyleCache::default(),
            css_target_fragment: None,
            viewport: Viewport::default(),
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
        }
    }

    /// Return the bounded diagnostics collected while parsing this document.
    pub fn diagnostics(&self) -> &[NativeDiagnostic] {
        &self.diagnostics
    }

    pub(crate) const fn compat_mode(&self) -> &'static str {
        self.quirks_mode.compat_mode()
    }

    pub(crate) const fn document_referrer_policy(&self) -> NativeFetchReferrerPolicy {
        self.document_referrer_policy
    }

    pub(crate) fn effective_base_url(&self, document_url: &str) -> String {
        let fallback = self.inherited_base_url.as_deref().unwrap_or(document_url);
        let Ok(fallback_url) = Url::parse(fallback) else {
            return document_url.to_owned();
        };
        let mut pending = vec![self.root];
        while let Some(node_id) = pending.pop() {
            let Some(node) = self.node(node_id) else {
                continue;
            };
            if node.element_name() == Some("base")
                && node.state.namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI)
                && self.is_attached(node_id)
                && let Some(href) = node.attribute("href")
            {
                return fallback_url
                    .join(href)
                    .map_or_else(|_| fallback_url.to_string(), |url| url.to_string());
            }
            pending.extend(node.children.iter().rev().copied());
        }
        fallback_url.to_string()
    }

    pub(crate) fn set_inherited_base_url(&mut self, base_url: Option<String>) {
        self.inherited_base_url = base_url.filter(|value| Url::parse(value).is_ok());
    }

    pub(crate) fn inherited_base_url(&self) -> Option<&str> {
        self.inherited_base_url.as_deref()
    }

    pub(crate) fn image_referrer_policy_for_node(
        &self,
        node_id: NativeNodeId,
    ) -> NativeFetchReferrerPolicy {
        let element_policy = self
            .node(node_id)
            .filter(|node| {
                (node.element_name() == Some("img")
                    && node.state.namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI)
                    || self.is_svg_image_node(node_id))
                    && self.is_attached(node_id)
            })
            .and_then(|node| node.attribute("referrerpolicy"))
            .and_then(parse_element_referrer_policy);
        element_policy.unwrap_or(self.document_referrer_policy)
    }

    pub(super) fn script_referrer_policy_for_node(
        &self,
        node_id: NativeNodeId,
    ) -> NativeFetchReferrerPolicy {
        let element_policy = self
            .node(node_id)
            .filter(|node| {
                node.element_name() == Some("script")
                    && node.state.namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI)
                    && self.is_attached(node_id)
            })
            .and_then(|node| node.attribute("referrerpolicy"))
            .and_then(parse_element_referrer_policy);
        element_policy.unwrap_or(self.document_referrer_policy)
    }

    pub(crate) fn stylesheet_link_referrer_policy_for_node_index(
        &self,
        node_index: u32,
    ) -> NativeFetchReferrerPolicy {
        let element_policy = self
            .nodes
            .get(node_index as usize)
            .filter(|node| {
                node.element_name() == Some("link")
                    && node.state.namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI)
                    && self.is_attached(node.id())
            })
            .and_then(|node| node.attribute("referrerpolicy"))
            .and_then(parse_element_referrer_policy);
        element_policy.unwrap_or(self.document_referrer_policy)
    }

    pub(super) fn apply_meta_referrer_policy_to_node(&mut self, id: NativeNodeId) {
        let policy = self.node(id).and_then(|node| {
            if !self.is_attached(id)
                || node.element_name() != Some("meta")
                || node.state.namespace_uri.as_deref() != Some(HTML_NAMESPACE_URI)
                || !node
                    .attribute("name")
                    .is_some_and(|name| name.eq_ignore_ascii_case("referrer"))
            {
                return None;
            }
            parse_meta_referrer_policy(node.attribute("content")?)
        });
        if let Some(policy) = policy {
            self.document_referrer_policy = policy;
        }
    }

    pub(super) fn apply_meta_referrer_policy_subtree(&mut self, root: NativeNodeId) {
        let mut pending = vec![root];
        let mut remaining = self.nodes.len();
        while let Some(id) = pending.pop() {
            if remaining == 0 {
                break;
            }
            remaining -= 1;
            let children = self
                .node(id)
                .map(|node| node.children().to_vec())
                .unwrap_or_default();
            self.apply_meta_referrer_policy_to_node(id);
            pending.extend(children.into_iter().rev());
        }
    }

    pub const fn diagnostics_truncated(&self) -> bool {
        self.diagnostics_truncated
    }

    pub const fn generation(&self) -> u32 {
        self.generation
    }

    /// Return the revision represented by this document's semantic references.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub const fn root(&self) -> NativeNodeId {
        self.root
    }

    pub fn node(&self, id: NativeNodeId) -> Option<&NativeNode> {
        let node = self.raw_node(id)?;
        (self.is_attached(id) || self.is_attached_template_content(id)).then_some(node)
    }

    pub const fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Atomically move one complete native node subtree and its caller-realm
    /// temporary identities to another browsing-context document. Both
    /// documents are drafted before either live document is replaced.
    #[cfg(test)]
    pub(crate) fn transfer_script_node_to(
        &mut self,
        destination: &mut Self,
        source_index: u32,
        identities: &[NativeNodeTransferIdentity],
    ) -> Result<(), NativeEngineError> {
        self.transfer_script_nodes_to(
            destination,
            &[NativeNodeSubtreeTransfer {
                source_index,
                identities: identities.to_vec(),
            }],
        )
    }

    /// Atomically move one or more disjoint subtrees and all retained
    /// temporary identities. The multi-root form represents virtual
    /// DocumentFragments without creating a synthetic native node.
    pub fn transfer_script_nodes_to(
        &mut self,
        destination: &mut Self,
        transfers: &[NativeNodeSubtreeTransfer],
    ) -> Result<(), NativeEngineError> {
        if transfers.is_empty() || transfers.len() > self.max_nodes {
            return Err(NativeEngineError::limit(
                "native node transfer roots",
                self.max_nodes,
                transfers.len(),
            ));
        }
        let mut source_draft = self.clone();
        let mut destination_draft = destination.clone();
        let source_revision = source_draft.revision.checked_add(1).ok_or_else(|| {
            NativeEngineError::TargetNotActionable {
                reason: "source document revision is exhausted".into(),
            }
        })?;
        let destination_revision = destination_draft.revision.checked_add(1).ok_or_else(|| {
            NativeEngineError::TargetNotActionable {
                reason: "destination document revision is exhausted".into(),
            }
        })?;
        let mut transferred_indices = BTreeSet::new();
        let mut total_identities = 0usize;
        for transfer in transfers {
            total_identities = total_identities.saturating_add(transfer.identities.len());
            if total_identities > source_draft.max_nodes {
                return Err(NativeEngineError::limit(
                    "native node transfer identities",
                    source_draft.max_nodes,
                    total_identities,
                ));
            }
            source_draft.transfer_script_node_to_draft(
                &mut destination_draft,
                transfer.source_index,
                &transfer.identities,
                &mut transferred_indices,
            )?;
        }
        source_draft.revision = source_revision;
        destination_draft.revision = destination_revision;
        *self = source_draft;
        *destination = destination_draft;
        Ok(())
    }

    pub(super) fn transfer_script_node_to_draft(
        &mut self,
        destination: &mut Self,
        source_index: u32,
        identities: &[NativeNodeTransferIdentity],
        transferred_indices: &mut BTreeSet<u32>,
    ) -> Result<(), NativeEngineError> {
        if identities.is_empty() || identities.len() > self.max_nodes {
            return Err(NativeEngineError::limit(
                "native node transfer identities",
                self.max_nodes,
                identities.len(),
            ));
        }

        let resolve_source = |index: u32| {
            self.script_node_ids
                .get(&index)
                .copied()
                .unwrap_or_else(|| NativeNodeId::from_parts(self.generation, index))
        };
        let root = resolve_source(source_index);
        let root_node = self
            .raw_node(root)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if matches!(&root_node.kind, NativeNodeKind::Document) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "a Document cannot be transferred with adoptNode".into(),
            });
        }
        if root_node.shadow_host.is_some() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "a ShadowRoot cannot be transferred with adoptNode".into(),
            });
        }

        let mut subtree = Vec::new();
        let mut seen = BTreeSet::new();
        let mut pending = vec![(root, 0usize)];
        while let Some((id, depth)) = pending.pop() {
            if id.generation != self.generation || !seen.insert(id.index) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "native node transfer subtree contains a stale or repeated node".into(),
                });
            }
            let node = self.raw_node(id).ok_or(NativeEngineError::DetachedTarget)?;
            if matches!(&node.kind, NativeNodeKind::Document) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "native node transfer subtree contains a Document".into(),
                });
            }
            if subtree.len() >= self.max_nodes || subtree.len() >= destination.max_nodes {
                return Err(NativeEngineError::limit(
                    "native node transfer subtree",
                    self.max_nodes.min(destination.max_nodes),
                    subtree.len().saturating_add(1),
                ));
            }
            if depth > self.max_dom_depth || depth > destination.max_dom_depth {
                return Err(NativeEngineError::limit(
                    "native node transfer depth",
                    self.max_dom_depth.min(destination.max_dom_depth),
                    depth,
                ));
            }
            subtree.push(id.index);
            let child_depth = depth.saturating_add(usize::from(node.element_name().is_some()));
            for child in node.children.iter().rev() {
                pending.push((*child, child_depth));
            }
            if let Some(content) = node.template_content {
                pending.push((content, child_depth));
            }
            if let Some(shadow_root) = node.shadow_root {
                pending.push((shadow_root, child_depth));
            }
        }

        for source_node_index in &subtree {
            if !transferred_indices.insert(*source_node_index) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "native node transfer roots overlap".into(),
                });
            }
        }

        let mut identity_by_source = BTreeMap::new();
        let mut destination_aliases = BTreeSet::new();
        for identity in identities {
            let source_id = resolve_source(identity.source_index);
            if !seen.contains(&source_id.index)
                || source_id.generation != self.generation
                || identity.destination_temporary_index < SCRIPT_TEMP_NODE_BASE
                || destination
                    .script_node_ids
                    .contains_key(&identity.destination_temporary_index)
                || !destination_aliases.insert(identity.destination_temporary_index)
                || identity_by_source
                    .insert(source_id.index, identity.destination_temporary_index)
                    .is_some()
            {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "native node transfer identity mapping is invalid".into(),
                });
            }
        }
        if identity_by_source.len() != subtree.len() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: format!(
                    "native node transfer identity mapping covers {} of {} subtree nodes",
                    identity_by_source.len(),
                    subtree.len(),
                ),
            });
        }
        for (temporary_index, node_id) in &self.script_node_ids {
            if seen.contains(&node_id.index) && !identity_by_source.contains_key(&node_id.index) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: format!(
                        "native node transfer omitted retained source identity {temporary_index}"
                    ),
                });
            }
        }

        let next_len = destination
            .nodes
            .len()
            .checked_add(subtree.len())
            .ok_or_else(|| NativeEngineError::limit("native DOM nodes", 0, usize::MAX))?;
        if next_len > destination.max_nodes {
            return Err(NativeEngineError::limit(
                "native DOM nodes",
                destination.max_nodes,
                next_len,
            ));
        }
        let target_revision = destination.revision.checked_add(1).ok_or_else(|| {
            NativeEngineError::TargetNotActionable {
                reason: "destination document revision is exhausted".into(),
            }
        })?;
        let source_revision =
            self.revision
                .checked_add(1)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "source document revision is exhausted".into(),
                })?;

        let mut native_index_map = BTreeMap::new();
        for (offset, source_node_index) in subtree.iter().copied().enumerate() {
            let destination_index = u32::try_from(destination.nodes.len().saturating_add(offset))
                .map_err(|_| {
                NativeEngineError::limit("native DOM node index", u32::MAX as usize, offset)
            })?;
            native_index_map.insert(source_node_index, destination_index);
        }
        let target_id = |source_id: NativeNodeId| {
            native_index_map
                .get(&source_id.index)
                .copied()
                .map(|index| NativeNodeId::from_parts(destination.generation, index))
        };

        let root_parent = self.raw_node(root).and_then(NativeNode::parent);
        if let Some(parent) = root_parent {
            let parent_node = self
                .raw_node_mut(parent)
                .ok_or(NativeEngineError::DetachedTarget)?;
            parent_node.children.retain(|child| *child != root);
        }
        if let Some(template_host) = self.raw_node(root).and_then(|node| node.template_host)
            && !seen.contains(&template_host.index)
            && let Some(host) = self.raw_node_mut(template_host)
            && host.template_content == Some(root)
        {
            host.template_content = None;
        }

        let mut moved_nodes = Vec::with_capacity(subtree.len());
        for source_node_index in &subtree {
            let source_id = NativeNodeId::from_parts(self.generation, *source_node_index);
            let mut node = self
                .raw_node(source_id)
                .cloned()
                .ok_or(NativeEngineError::DetachedTarget)?;
            let next_index = native_index_map[source_node_index];
            node.id = NativeNodeId::from_parts(destination.generation, next_index);
            node.parent = match node.parent {
                Some(_) if source_id == root => None,
                Some(parent) => Some(target_id(parent).ok_or_else(|| {
                    NativeEngineError::TargetNotActionable {
                        reason: "native node transfer subtree has an external parent".into(),
                    }
                })?),
                None => None,
            };
            node.children = node
                .children
                .iter()
                .map(|child| {
                    target_id(*child).ok_or_else(|| NativeEngineError::TargetNotActionable {
                        reason: "native node transfer subtree has an external child".into(),
                    })
                })
                .collect::<Result<_, _>>()?;
            node.template_content = node
                .template_content
                .map(|content| {
                    target_id(content).ok_or_else(|| NativeEngineError::TargetNotActionable {
                        reason: "native template transfer content is outside its subtree".into(),
                    })
                })
                .transpose()?;
            node.template_host = match node.template_host {
                Some(host) => match target_id(host) {
                    Some(target_host) => Some(target_host),
                    None if source_id == root => None,
                    None => {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "native template transfer host is outside its subtree".into(),
                        });
                    }
                },
                None => None,
            };
            node.shadow_root = node
                .shadow_root
                .map(|shadow_root| {
                    target_id(shadow_root).ok_or_else(|| NativeEngineError::TargetNotActionable {
                        reason: "native shadow root is outside its transferred host subtree".into(),
                    })
                })
                .transpose()?;
            node.shadow_host = node
                .shadow_host
                .map(|shadow_host| {
                    target_id(shadow_host).ok_or_else(|| NativeEngineError::TargetNotActionable {
                        reason: "native shadow host is outside its transferred root subtree".into(),
                    })
                })
                .transpose()?;
            moved_nodes.push(node);
        }

        let mut moved_started_scripts = Vec::new();
        for source_node_index in &subtree {
            if self.started_script_nodes.remove(source_node_index)
                && let Some(destination_index) = native_index_map.get(source_node_index)
            {
                moved_started_scripts.push(*destination_index);
            }
            self.processed_csp_meta_nodes.remove(source_node_index);
            self.pending_csp_meta_policies.remove(source_node_index);
        }
        destination
            .started_script_nodes
            .extend(moved_started_scripts);

        move_node_indexed_entries(
            &mut self.external_stylesheet_states,
            &mut destination.external_stylesheet_states,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.image_resources,
            &mut destination.image_resources,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.image_loads,
            &mut destination.image_loads,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.video_poster_resources,
            &mut destination.video_poster_resources,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.video_poster_loads,
            &mut destination.video_poster_loads,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.media_resources,
            &mut destination.media_resources,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.media_loads,
            &mut destination.media_loads,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.media_errors,
            &mut destination.media_errors,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.background_image_resources,
            &mut destination.background_image_resources,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.canvas_resources,
            &mut destination.canvas_resources,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.inline_style_element_reports,
            &mut destination.inline_style_element_reports,
            &native_index_map,
        );
        move_node_indexed_entries(
            &mut self.inline_style_attribute_reports,
            &mut destination.inline_style_attribute_reports,
            &native_index_map,
        );

        for source_node_index in &subtree {
            let tombstone_id = NativeNodeId::from_parts(self.generation, *source_node_index);
            let tombstone = self
                .raw_node_mut(tombstone_id)
                .ok_or(NativeEngineError::DetachedTarget)?;
            tombstone.parent = None;
            tombstone.children.clear();
            tombstone.template_content = None;
            tombstone.template_host = None;
            tombstone.shadow_root = None;
            tombstone.shadow_host = None;
            tombstone.shadow_mode = None;
            tombstone.kind = NativeNodeKind::DocumentFragment;
            tombstone.state = NativeElementState::default();
        }
        self.script_node_ids
            .retain(|_, id| !seen.contains(&id.index));

        for node in moved_nodes {
            destination.nodes.push(node);
        }
        for (source_node_index, temporary_index) in identity_by_source {
            let destination_index = native_index_map[&source_node_index];
            destination.script_node_ids.insert(
                temporary_index,
                NativeNodeId::from_parts(destination.generation, destination_index),
            );
        }
        self.revision = source_revision;
        destination.revision = target_revision;
        self.invalidate_layout_styles();
        destination.invalidate_layout_styles();
        Ok(())
    }

    /// Return a revision-bound reference for a current arena node.
    pub fn node_reference(&self, id: NativeNodeId) -> Option<String> {
        self.node(id)
            .map(|_| format!("ref=r{}:n{}", self.revision, id.index))
    }

    /// Return a bounded semantic projection in deterministic document order.
    pub fn semantic_nodes(&self) -> Vec<NativeSemanticNode> {
        let hidden_by_index = self.effective_hidden_by_index();
        self.nodes
            .iter()
            .filter_map(|node| {
                let hidden = hidden_by_index
                    .get(&node.id.index())
                    .copied()
                    .unwrap_or_else(|| self.is_hidden(node.id));
                self.semantic_node_with_hidden(node.id, hidden)
            })
            .collect()
    }

    pub(super) fn parent_element_index(&self, id: NativeNodeId) -> Option<u32> {
        let mut parent = self.node(id).and_then(NativeNode::parent);
        while let Some(parent_id) = parent {
            let parent_node = self.node(parent_id)?;
            if parent_node.element_name().is_some() {
                return Some(parent_id.index());
            }
            parent = parent_node.parent();
        }
        None
    }

    /// Derive the current document's bounded integer-pixel layout.
    pub fn layout(&self, viewport: Viewport) -> Result<NativeLayoutSnapshot, NativeEngineError> {
        if let Some(layout) = self.layout_style_cache.layout(self.revision, viewport) {
            return Ok(layout);
        }
        let layout = NativeLayoutSnapshot::compute(self, viewport)?;
        self.layout_style_cache.cache_layout(layout.clone());
        Ok(layout)
    }

    pub(in crate::native_engine) fn layout_text_width_cache(
        &self,
    ) -> std::collections::HashMap<
        super::super::layout::NativeTextWidthCacheKey,
        std::collections::HashMap<String, u32>,
    > {
        self.layout_style_cache.text_widths()
    }

    pub(in crate::native_engine) fn cache_layout_text_widths(
        &self,
        widths: std::collections::HashMap<
            super::super::layout::NativeTextWidthCacheKey,
            std::collections::HashMap<String, u32>,
        >,
        hits: usize,
    ) {
        self.layout_style_cache.cache_text_widths(widths, hits);
    }

    #[cfg(test)]
    pub(crate) fn layout_text_width_cache_hits(&self) -> usize {
        self.layout_style_cache.text_width_cache_hits()
    }

    /// Hit test one point in the supplied viewport without scrolling or
    /// adjusting the requested coordinates.
    pub fn hit_test(
        &self,
        viewport: Viewport,
        x: i64,
        y: i64,
    ) -> Result<Option<NativeNodeId>, NativeEngineError> {
        let layout = self.layout(viewport)?;
        self.hit_test_with_layout(&layout, x, y)
    }

    pub(crate) fn hit_test_with_layout(
        &self,
        layout: &NativeLayoutSnapshot,
        x: i64,
        y: i64,
    ) -> Result<Option<NativeNodeId>, NativeEngineError> {
        let Some(hit) = layout.hit_test(x, y)? else {
            return Ok(None);
        };
        Ok(self
            .image_map_area_at_point(layout, hit, x, y)
            .or(Some(hit)))
    }

    pub(super) fn image_map_area_at_point(
        &self,
        layout: &NativeLayoutSnapshot,
        image_id: NativeNodeId,
        x: i64,
        y: i64,
    ) -> Option<NativeNodeId> {
        if self.node(image_id)?.element_name() != Some("img") {
            return None;
        }
        let map_id = self.image_map_for_image(image_id)?;
        let local = image_content_point(layout, image_id, x, y)?;
        let mut pending = self
            .node(map_id)?
            .children()
            .iter()
            .rev()
            .copied()
            .collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            let node = self.node(id)?;
            if node.element_name() == Some("area")
                && image_map_shape_contains(node, local.0, local.1)
            {
                return Some(id);
            }
            pending.extend(node.children().iter().rev().copied());
        }
        None
    }

    pub(super) fn image_map_for_image(&self, image_id: NativeNodeId) -> Option<NativeNodeId> {
        let reference = self.node(image_id)?.attribute("usemap")?;
        let hash = reference.find('#')?;
        let name = reference.get(hash + 1..)?;
        if name.is_empty() {
            return None;
        }
        let mut pending = vec![self.root];
        while let Some(id) = pending.pop() {
            let node = self.node(id)?;
            if node.element_name() == Some("map")
                && (node.attribute("id") == Some(name) || node.attribute("name") == Some(name))
            {
                return Some(id);
            }
            pending.extend(node.children().iter().rev().copied());
        }
        None
    }

    pub(super) fn image_map_for_area(&self, area_id: NativeNodeId) -> Option<NativeNodeId> {
        let mut current = self.node(area_id)?.parent();
        while let Some(id) = current {
            let node = self.node(id)?;
            if node.element_name() == Some("map") {
                return Some(id);
            }
            current = node.parent();
        }
        None
    }

    pub(super) fn image_map_area_focus_anchors(&self, area_id: NativeNodeId) -> Vec<NativeNodeId> {
        let Some(area) = self.node(area_id) else {
            return Vec::new();
        };
        let Some(map_id) = self.image_map_for_area(area_id) else {
            return Vec::new();
        };
        if area.element_name() != Some("area")
            || !self.is_hyperlink_element(area_id)
            || !image_map_area_has_nonempty_shape(area)
            || self.is_hidden(area_id)
            || self.is_disabled(area_id)
        {
            return Vec::new();
        }

        let mut anchors = Vec::new();
        let mut pending = vec![self.root];
        while let Some(id) = pending.pop() {
            let Some(node) = self.node(id) else {
                continue;
            };
            if node.element_name() == Some("img")
                && self.is_attached(id)
                && !self.is_hidden_for_layout(id)
                && self.image_map_for_image(id) == Some(map_id)
            {
                anchors.push(id);
            }
            pending.extend(node.children().iter().rev().copied());
        }
        anchors
    }

    pub(crate) fn keyboard_event_target(&self, focused_node: NativeNodeId) -> NativeNodeId {
        if self.node(focused_node).and_then(NativeNode::element_name) != Some("area") {
            return focused_node;
        }
        let anchors = self.image_map_area_focus_anchors(focused_node);
        let selected = self
            .node(focused_node)
            .and_then(|node| node.state.focus_anchor_node_index)
            .and_then(|index| {
                anchors
                    .iter()
                    .find(|anchor| anchor.index() == index)
                    .copied()
            });
        selected
            .or_else(|| anchors.first().copied())
            .unwrap_or(focused_node)
    }

    pub(crate) fn image_map_area_has_visible_image(
        &self,
        area_id: NativeNodeId,
        layout: &NativeLayoutSnapshot,
    ) -> bool {
        self.image_map_area_viewport_rect(area_id, layout).is_some()
    }

    pub(crate) fn image_map_area_viewport_rect(
        &self,
        area_id: NativeNodeId,
        layout: &NativeLayoutSnapshot,
    ) -> Option<NativeRect> {
        let map_id = self.image_map_for_area(area_id)?;
        self.nodes.iter().find_map(|node| {
            let image_id = node.id();
            if node.element_name() != Some("img")
                || !self.is_attached(image_id)
                || self.image_map_for_image(image_id) != Some(map_id)
                || self.is_hidden_for_layout(image_id)
            {
                return None;
            }
            layout
                .viewport_rect_for(image_id)
                .filter(|rect| rect.width > 0 && rect.height > 0)
        })
    }

    /// Derive an immutable display list from the current layout revision.
    pub fn display_list(&self, viewport: Viewport) -> Result<NativeDisplayList, NativeEngineError> {
        let layout = self.layout(viewport)?;
        NativeDisplayList::build(self, &layout)
    }

    /// Derive and replay the current document into a bounded logical surface.
    pub fn rasterize(&self, viewport: Viewport) -> Result<NativeSurface, NativeEngineError> {
        self.display_list(viewport)?.rasterize()
    }

    /// Resolve one explicit semantic locator to exactly one current element.
    pub fn resolve_target(&self, locator: &str) -> Result<NativeNodeId, NativeEngineError> {
        let locator = parse_locator(locator)?;
        match locator {
            NativeLocator::Reference { revision, index } => {
                if revision != self.revision {
                    return Err(NativeEngineError::DetachedTarget);
                }
                let id = NativeNodeId {
                    generation: self.generation,
                    index,
                };
                self.node(id)
                    .filter(|node| node.element_name().is_some())
                    .map(|node| node.id())
                    .ok_or(NativeEngineError::TargetNotFound)
            }
            NativeLocator::Id(value) => {
                self.unique_element_matches(|node| node.attribute("id") == Some(value))
            }
            NativeLocator::Role { role, name } => self.unique_semantic_matches(|node| {
                node.role == role && name.is_none_or(|expected| node.name == expected)
            }),
            NativeLocator::Name(value) => self.unique_semantic_matches(|node| node.name == value),
            NativeLocator::Text(value) => self.unique_semantic_matches(|node| {
                self.element_text(node.node_id, MAX_LOCATOR_BYTES)
                    .is_some_and(|(text, truncated)| !truncated && text == value)
            }),
            NativeLocator::Css(selector) => unique_match(
                super::super::css::selector_matches_in_document(self, selector)?,
            ),
        }
    }

    pub(super) fn reset_media_load(
        &mut self,
        node_index: u32,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let node_id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node(node_id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("audio") && node.element_name() != Some("video") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "media load requires an audio or video element".into(),
            });
        }
        let source = self.selected_media_source(node_id);
        self.media_resources.remove(&node_index);
        self.media_errors.remove(&node_index);
        self.media_loads.remove(&node_index);
        if let Some(source) = source.filter(|source| !source.is_empty()) {
            self.media_loads.insert(node_index, source);
        }
        Ok(())
    }

    pub(super) fn add_node(
        &mut self,
        parent: NativeNodeId,
        kind: NativeNodeKind,
        max_nodes: usize,
    ) -> Result<NativeNodeId, NativeEngineError> {
        if self.raw_node(parent).is_none() {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder referenced an unknown parent".into(),
            });
        }
        if self.nodes.len() >= max_nodes {
            return Err(NativeEngineError::limit(
                "DOM nodes",
                max_nodes,
                self.nodes.len().saturating_add(1),
            ));
        }
        let index = u32::try_from(self.nodes.len()).map_err(|_| {
            NativeEngineError::limit("DOM node index", u32::MAX as usize, self.nodes.len())
        })?;
        let id = NativeNodeId {
            generation: self.generation,
            index,
        };
        let state = match &kind {
            NativeNodeKind::Element { name, attributes } => {
                NativeElementState::initial(name, attributes)
            }
            NativeNodeKind::Document
            | NativeNodeKind::DocumentFragment
            | NativeNodeKind::DocumentType { .. }
            | NativeNodeKind::Comment(_)
            | NativeNodeKind::Text(_) => NativeElementState::default(),
        };
        self.nodes.push(NativeNode {
            id,
            parent: Some(parent),
            children: Vec::new(),
            template_content: None,
            template_host: None,
            shadow_host: None,
            shadow_root: None,
            shadow_mode: None,
            kind,
            state,
        });
        let Some(parent_node) = self.raw_node_mut(parent) else {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder referenced an unknown parent".into(),
            });
        };
        parent_node.children.push(id);
        Ok(id)
    }

    #[cfg(test)]
    pub(super) fn add_node_before(
        &mut self,
        parent: NativeNodeId,
        before: NativeNodeId,
        kind: NativeNodeKind,
        max_nodes: usize,
    ) -> Result<NativeNodeId, NativeEngineError> {
        if self.raw_node(before).and_then(NativeNode::parent) != Some(parent) {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "foster-parent table is not a child of its insertion parent".into(),
            });
        }
        let id = self.add_node(parent, kind, max_nodes)?;
        let parent_node = self
            .raw_node_mut(parent)
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder lost a foster-parent insertion parent".into(),
            })?;
        let child_index = parent_node
            .children
            .iter()
            .position(|child| *child == id)
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder lost a foster-parented node".into(),
            })?;
        let before_index = parent_node
            .children
            .iter()
            .position(|child| *child == before)
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder lost the foster-parent table".into(),
            })?;
        let child = parent_node.children.remove(child_index);
        parent_node.children.insert(before_index, child);
        Ok(id)
    }

    pub(super) fn add_detached_node(
        &mut self,
        kind: NativeNodeKind,
        max_nodes: usize,
    ) -> Result<NativeNodeId, NativeEngineError> {
        if self.nodes.len() >= max_nodes {
            return Err(NativeEngineError::limit(
                "DOM nodes",
                max_nodes,
                self.nodes.len().saturating_add(1),
            ));
        }
        let index = u32::try_from(self.nodes.len()).map_err(|_| {
            NativeEngineError::limit("DOM node index", u32::MAX as usize, self.nodes.len())
        })?;
        let id = NativeNodeId {
            generation: self.generation,
            index,
        };
        let state = match &kind {
            NativeNodeKind::Element { name, attributes } => {
                NativeElementState::initial(name, attributes)
            }
            NativeNodeKind::Document
            | NativeNodeKind::DocumentFragment
            | NativeNodeKind::DocumentType { .. }
            | NativeNodeKind::Comment(_)
            | NativeNodeKind::Text(_) => NativeElementState::default(),
        };
        self.nodes.push(NativeNode {
            id,
            parent: None,
            children: Vec::new(),
            template_content: None,
            template_host: None,
            shadow_host: None,
            shadow_root: None,
            shadow_mode: None,
            kind,
            state,
        });
        Ok(id)
    }

    pub(super) fn raw_node(&self, id: NativeNodeId) -> Option<&NativeNode> {
        (id.generation == self.generation)
            .then(|| self.nodes.get(id.index as usize))
            .flatten()
    }

    pub(super) fn raw_node_mut(&mut self, id: NativeNodeId) -> Option<&mut NativeNode> {
        (id.generation == self.generation)
            .then(|| self.nodes.get_mut(id.index as usize))
            .flatten()
    }

    pub(super) fn is_attached(&self, id: NativeNodeId) -> bool {
        if id.generation != self.generation {
            return false;
        }
        let mut current = id;
        for _ in 0..=self.nodes.len() {
            if current == self.root {
                return true;
            }
            let Some(parent) = self
                .raw_node(current)
                .and_then(|node| node.parent().or(node.shadow_host))
            else {
                return false;
            };
            if parent == current {
                return false;
            }
            current = parent;
        }
        false
    }

    pub(super) fn is_attached_template_content(&self, id: NativeNodeId) -> bool {
        let mut current = id;
        for _ in 0..=self.nodes.len() {
            let Some(node) = self.raw_node(current) else {
                return false;
            };
            if current == self.root {
                return true;
            }
            if let Some(parent) = node.parent {
                current = parent;
                continue;
            }
            let Some(host) = node.template_host else {
                return false;
            };
            if self
                .raw_node(host)
                .is_none_or(|host_node| host_node.template_content != Some(current))
            {
                return false;
            }
            current = host;
        }
        false
    }

    pub(super) fn belongs_to_template_content(&self, id: NativeNodeId) -> bool {
        let mut current = id;
        for _ in 0..=self.nodes.len() {
            let Some(node) = self.raw_node(current) else {
                return false;
            };
            if let Some(host) = node.template_host {
                return self
                    .raw_node(host)
                    .is_some_and(|host_node| host_node.template_content == Some(current));
            }
            if let Some(parent) = node.parent {
                current = parent;
                continue;
            }
            return false;
        }
        false
    }

    pub(super) fn is_descendant_of_element(&self, id: NativeNodeId, wanted: &str) -> bool {
        let mut current = self.raw_node(id).and_then(NativeNode::parent);
        for _ in 0..=self.nodes.len() {
            let Some(current_id) = current else {
                return false;
            };
            let Some(node) = self.raw_node(current_id) else {
                return false;
            };
            if node.element_name() == Some(wanted) {
                return true;
            }
            current = node.parent();
        }
        false
    }

    pub(super) fn node_mut(&mut self, id: NativeNodeId) -> Option<&mut NativeNode> {
        self.is_attached(id)
            .then(|| self.raw_node_mut(id))
            .flatten()
    }

    pub(super) fn resolve_script_node_id(
        &self,
        node_index: u32,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> NativeNodeId {
        script_nodes
            .get(&node_index)
            .copied()
            .unwrap_or_else(|| NativeNodeId::from_parts(self.generation, node_index))
    }

    pub(super) fn script_node<'a>(
        &'a self,
        id: NativeNodeId,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Option<&'a NativeNode> {
        if script_nodes.values().any(|candidate| *candidate == id)
            || self.is_attached(id)
            || self.belongs_to_template_content(id)
        {
            self.raw_node(id)
        } else {
            None
        }
    }

    pub(super) fn script_node_mut<'a>(
        &'a mut self,
        id: NativeNodeId,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Option<&'a mut NativeNode> {
        if script_nodes.values().any(|candidate| *candidate == id)
            || self.is_attached(id)
            || self.belongs_to_template_content(id)
        {
            self.raw_node_mut(id)
        } else {
            None
        }
    }

    pub(super) fn detach_subtree(&mut self, id: NativeNodeId) -> Result<(), NativeEngineError> {
        if self.is_attached(id) {
            self.record_removed_embedded_frame_owners(id);
        }
        let parent = self
            .raw_node(id)
            .and_then(NativeNode::parent)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let Some(parent_node) = self.raw_node_mut(parent) else {
            return Err(NativeEngineError::DetachedTarget);
        };
        parent_node.children.retain(|child| *child != id);

        // A removed node is detached from the live document, but the node
        // object keeps its descendants. Script code can retain and later
        // reinsert that object, and cross-document adoptNode needs the
        // detached subtree intact while its queued host transfer is applied.
        let mut pending = vec![id];
        let mut detached = Vec::new();
        while let Some(current) = pending.pop() {
            let node = self
                .raw_node(current)
                .ok_or(NativeEngineError::DetachedTarget)?;
            let children = node.children.clone();
            pending.extend(children);
            detached.push(current);
        }
        for current in &detached {
            self.canvas_resources.remove(&current.index());
            self.media_resources.remove(&current.index());
            self.media_loads.remove(&current.index());
            self.media_errors.remove(&current.index());
        }
        if let Some(root) = self.raw_node_mut(id) {
            root.parent = None;
        }
        for current in detached {
            if let Some(node) = self.raw_node_mut(current) {
                node.state.focused = false;
            }
        }
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(super) fn record_removed_embedded_frame_owners(&mut self, root: NativeNodeId) {
        let mut pending = vec![root];
        while let Some(current) = pending.pop() {
            let Some((is_frame_owner, children)) = self.raw_node(current).map(|node| {
                (
                    matches!(node.element_name(), Some("iframe" | "frame")),
                    node.children.clone(),
                )
            }) else {
                continue;
            };
            if is_frame_owner {
                self.next_embedded_frame_owner_removal_sequence = self
                    .next_embedded_frame_owner_removal_sequence
                    .saturating_add(1);
                self.embedded_frame_owner_removal_sequences.insert(
                    current.index(),
                    self.next_embedded_frame_owner_removal_sequence,
                );
            }
            pending.extend(children);
        }
    }

    pub(super) fn append_script_child(
        &mut self,
        parent: NativeNodeId,
        child: NativeNodeId,
        before: Option<NativeNodeId>,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let parent_node = self
            .script_node(parent, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let parent_is_document = matches!(parent_node.kind(), NativeNodeKind::Document);
        let parent_is_fragment = matches!(parent_node.kind(), NativeNodeKind::DocumentFragment);
        if !parent_is_document
            && !parent_is_fragment
            && !matches!(parent_node.kind(), NativeNodeKind::Element { .. })
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "appendChild parent must be an element, document, or fragment".into(),
            });
        }
        let child_node = self
            .script_node(child, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let child_is_document_type =
            matches!(child_node.kind(), NativeNodeKind::DocumentType { .. });
        if !matches!(
            child_node.kind(),
            NativeNodeKind::Element { .. } | NativeNodeKind::Text(_) | NativeNodeKind::Comment(_)
        ) && !(parent_is_document && child_is_document_type)
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "appendChild cannot insert a document node".into(),
            });
        }
        if child_is_document_type {
            if !parent_is_document {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "document type must be a child of the document".into(),
                });
            }
            if parent_node
                .children()
                .iter()
                .filter_map(|id| self.script_node(*id, script_nodes))
                .any(|node| {
                    matches!(node.kind(), NativeNodeKind::DocumentType { .. }) && node.id() != child
                })
            {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "document cannot have more than one document type".into(),
                });
            }
        } else if parent_is_document {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "document only accepts a document type in this native surface".into(),
            });
        }
        if child == self.root || parent == child {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "appendChild would create a DOM hierarchy cycle".into(),
            });
        }
        if before == Some(child) {
            return Ok(());
        }
        if let Some(before) = before {
            let before_node = self
                .script_node(before, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?;
            if before_node.parent() != Some(parent) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "insertBefore reference is not a child of the parent".into(),
                });
            }
        }

        let mut ancestor = Some(parent);
        for _ in 0..=self.nodes.len() {
            if ancestor == Some(child) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "appendChild would create a DOM hierarchy cycle".into(),
                });
            }
            let Some(current) = ancestor else {
                break;
            };
            ancestor = self.raw_node(current).and_then(NativeNode::parent);
        }

        let old_parent = self.raw_node(child).and_then(NativeNode::parent);
        if let Some(old_parent) = old_parent {
            if self.is_attached(child) {
                self.record_removed_embedded_frame_owners(child);
            }
            let Some(old_parent_node) = self.raw_node_mut(old_parent) else {
                return Err(NativeEngineError::DetachedTarget);
            };
            old_parent_node
                .children
                .retain(|candidate| *candidate != child);
        }
        let Some(child_node) = self.raw_node_mut(child) else {
            return Err(NativeEngineError::DetachedTarget);
        };
        child_node.parent = Some(parent);
        let Some(parent_node) = self.raw_node_mut(parent) else {
            return Err(NativeEngineError::DetachedTarget);
        };
        let insertion_index = before
            .and_then(|before| parent_node.children.iter().position(|id| *id == before))
            .unwrap_or(parent_node.children.len());
        parent_node.children.insert(insertion_index, child);
        self.invalidate_layout_styles();
        self.capture_attached_content_security_policy_meta();
        self.apply_meta_referrer_policy_subtree(child);
        Ok(())
    }

    pub(super) fn normalize_select_defaults(&mut self) {
        let select_ids = self
            .nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
            .filter(|node| node.element_name() == Some("select"))
            .map(NativeNode::id)
            .collect::<Vec<_>>();
        for select_id in select_ids {
            let option_ids = self.select_option_ids(select_id);
            if self
                .node(select_id)
                .is_some_and(|node| node.attribute("multiple").is_some())
            {
                continue;
            }
            let selected_id = option_ids
                .iter()
                .find(|option_id| {
                    self.node(**option_id)
                        .is_some_and(|node| node.state.selected)
                })
                .copied()
                .or_else(|| option_ids.first().copied());
            for option_id in option_ids {
                if let Some(node) = self.node_mut(option_id) {
                    node.state.selected = selected_id == Some(option_id);
                }
            }
        }
    }

    pub(super) fn select_for_option(
        &self,
        option_id: NativeNodeId,
    ) -> Result<NativeNodeId, NativeEngineError> {
        self.find_ancestor_element(option_id, "select")
            .ok_or_else(|| NativeEngineError::TargetNotActionable {
                reason: "option must belong to a select control".into(),
            })
    }

    pub(super) fn option_value(&self, option_id: NativeNodeId) -> Option<String> {
        let option = self.node(option_id)?;
        option.attribute("value").map(str::to_owned).or_else(|| {
            self.element_text(option_id, MAX_LOCATOR_BYTES)
                .map(|(value, _)| value)
        })
    }

    pub(super) fn select_option_ids(&self, select_id: NativeNodeId) -> Vec<NativeNodeId> {
        let mut options = Vec::new();
        let Some(select) = self.node(select_id) else {
            return options;
        };
        for child in select.children() {
            self.collect_select_options_in_tree(*child, &mut options, 0);
        }
        options
    }

    pub(super) fn collect_select_options_in_tree(
        &self,
        current_id: NativeNodeId,
        options: &mut Vec<NativeNodeId>,
        depth: usize,
    ) {
        if depth > MAX_NATIVE_DOM_DEPTH {
            return;
        }
        let Some(node) = self.node(current_id) else {
            return;
        };
        if node.element_name() == Some("option") {
            options.push(current_id);
        }
        for child in node.children() {
            self.collect_select_options_in_tree(*child, options, depth + 1);
        }
    }

    pub(super) fn is_descendant_of(&self, id: NativeNodeId, ancestor: NativeNodeId) -> bool {
        let mut parent = self.node(id).and_then(NativeNode::parent);
        while let Some(parent_id) = parent {
            if parent_id == ancestor {
                return true;
            }
            parent = self.node(parent_id).and_then(NativeNode::parent);
        }
        false
    }

    pub(super) fn find_ancestor_element(
        &self,
        id: NativeNodeId,
        wanted: &str,
    ) -> Option<NativeNodeId> {
        let mut parent = self.node(id).and_then(NativeNode::parent);
        while let Some(parent_id) = parent {
            let parent_node = self.node(parent_id)?;
            if parent_node.element_name() == Some(wanted) {
                return Some(parent_id);
            }
            parent = parent_node.parent();
        }
        None
    }

    pub(super) fn mark_user_interacted(
        &mut self,
        id: NativeNodeId,
    ) -> Result<(), NativeEngineError> {
        self.node_mut(id)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state
            .user_interacted = true;
        self.invalidate_layout_styles();
        Ok(())
    }
    pub(super) fn focus_element(
        &mut self,
        id: NativeNodeId,
    ) -> Vec<(NativeNodeId, NativeEventKind)> {
        self.focus_element_with_anchor(id, None)
    }

    pub(super) fn focus_element_with_anchor(
        &mut self,
        id: NativeNodeId,
        focus_anchor: Option<NativeNodeId>,
    ) -> Vec<(NativeNodeId, NativeEventKind)> {
        let focused_ids = self
            .nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
            .filter(|node| node.state.focused && node.id() != id)
            .map(NativeNode::id)
            .collect::<Vec<_>>();
        let focus_changes = !focused_ids.is_empty()
            || self.node(id).is_some_and(|node| {
                !node.state.focused
                    || node.state.focus_anchor_node_index != focus_anchor.map(NativeNodeId::index)
            });
        let mut events = Vec::new();
        for focused_id in focused_ids {
            if let Some(node) = self.node_mut(focused_id) {
                node.state.focused = false;
                node.state.focus_anchor_node_index = None;
                events.push((focused_id, NativeEventKind::Blur));
                events.push((focused_id, NativeEventKind::FocusOut));
            }
        }
        if let Some(node) = self.node_mut(id) {
            let focus_changed = !node.state.focused
                || node.state.focus_anchor_node_index != focus_anchor.map(NativeNodeId::index);
            node.state.focused = true;
            node.state.focus_anchor_node_index = focus_anchor.map(NativeNodeId::index);
            if focus_changed {
                events.push((id, NativeEventKind::Focus));
                events.push((id, NativeEventKind::FocusIn));
            }
        }
        self.initialize_selection_if_needed(id);
        if focus_changes {
            self.invalidate_layout_styles();
        }
        events
    }

    pub(super) fn unique_element_matches(
        &self,
        predicate: impl Fn(&NativeNode) -> bool,
    ) -> Result<NativeNodeId, NativeEngineError> {
        let matches = self
            .nodes
            .iter()
            .filter(|node| node.element_name().is_some() && predicate(node))
            .map(NativeNode::id)
            .collect::<Vec<_>>();
        unique_match(matches)
    }

    pub(super) fn unique_semantic_matches(
        &self,
        predicate: impl Fn(&NativeSemanticNode) -> bool,
    ) -> Result<NativeNodeId, NativeEngineError> {
        let matches = self
            .semantic_nodes()
            .into_iter()
            .filter(predicate)
            .map(|node| node.node_id)
            .collect::<Vec<_>>();
        unique_match(matches)
    }

    pub(super) fn semantic_node(&self, id: NativeNodeId) -> Option<NativeSemanticNode> {
        self.semantic_node_with_hidden(id, self.is_hidden(id))
    }

    fn semantic_node_with_hidden(
        &self,
        id: NativeNodeId,
        hidden: bool,
    ) -> Option<NativeSemanticNode> {
        let node = self.node(id)?;
        let tag_name = node.element_name()?.to_string();
        let role = self.semantic_role(id)?.to_string();
        let (name, name_truncated) = self.accessible_name(id, &role);
        let input_type = (tag_name == "input").then(|| {
            node.attribute("type")
                .unwrap_or("text")
                .to_ascii_lowercase()
        });
        let empty = matches!(role.as_str(), "textbox" | "combobox")
            .then(|| self.current_value(id).is_none_or(|value| value.is_empty()))
            .or_else(|| (role == "file").then_some(node.state.files.is_empty()));
        let checked = matches!(role.as_str(), "checkbox" | "radio").then(|| node.state.checked);
        let selected = (role == "option").then_some(node.state.selected);
        let navigation_target_fingerprint = (role == "link")
            .then(|| node.attribute("href"))
            .flatten()
            .map(|href| format!("{:x}", Sha256::digest(href.as_bytes())));
        Some(NativeSemanticNode {
            node_id: id,
            reference: self.node_reference(id)?,
            tag_name,
            role,
            name,
            name_truncated,
            input_type,
            empty,
            checked,
            selected,
            hidden,
            disabled: self.is_disabled(id),
            read_only: self.is_read_only(id),
            required: self.is_required(id),
            focused: node.state.focused,
            navigation_target_fingerprint,
        })
    }

    pub(crate) fn nearest_clickable_ancestor(&self, id: NativeNodeId) -> Option<NativeNodeId> {
        let mut current = Some(id);
        while let Some(current_id) = current {
            if let Some(semantic) = self.semantic_node(current_id)
                && matches!(
                    semantic.role.as_str(),
                    "button" | "link" | "checkbox" | "radio" | "textbox" | "combobox" | "option"
                )
            {
                return Some(current_id);
            }
            current = self.node(current_id).and_then(NativeNode::parent);
        }
        None
    }

    pub(crate) fn is_descendant_or_self(&self, id: NativeNodeId, ancestor: NativeNodeId) -> bool {
        let mut current = Some(id);
        while let Some(current_id) = current {
            if current_id == ancestor {
                return true;
            }
            current = self.node(current_id).and_then(NativeNode::parent);
        }
        false
    }

    pub(crate) fn link_href(&self, id: NativeNodeId) -> Option<&str> {
        let node = self.node(id)?;
        self.is_hyperlink_element(id)
            .then(|| node.attribute("href"))
            .flatten()
    }

    pub(crate) fn link_download_attribute(&self, id: NativeNodeId) -> Option<&str> {
        let node = self.node(id)?;
        if !self.is_hyperlink_element(id) {
            return None;
        }
        let attributes = node.attributes()?;
        attributes
            .contains_key("download")
            .then(|| node.attribute("download").unwrap_or(""))
    }

    /// Return whether an anchor explicitly requests a fresh browsing context.
    ///
    /// Named browsing contexts are intentionally kept out of this bounded
    /// topology projection until they have a stable public identity. The
    /// reserved `_blank` target is the interoperable new-page contract used by
    /// the native target owner today.
    pub(crate) fn link_opens_new_target(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        self.is_hyperlink_element(id)
            && node
                .attribute("target")
                .is_some_and(|target| target.trim().eq_ignore_ascii_case("_blank"))
    }

    pub(super) fn is_hyperlink_element(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        node.attribute("href").is_some()
            && match node.element_name() {
                Some("a") => true,
                Some("area") => self.image_map_for_area(id).is_some(),
                _ => false,
            }
    }

    /// Resolve one decoded, exact local fragment target. A unique `id` wins;
    /// when no `id` matches, a unique legacy `<a name>` anchor is accepted.
    /// Duplicate targets are rejected so the bounded engine never invents
    /// browser recovery rules.
    pub(crate) fn fragment_target(&self, fragment: &str) -> Option<NativeNodeId> {
        if fragment.is_empty() {
            return None;
        }
        let mut target = None;
        for node in &self.nodes {
            if !self.is_attached(node.id())
                || node.element_name().is_none()
                || node.attribute("id") != Some(fragment)
            {
                continue;
            }
            if target.is_some() {
                return None;
            }
            target = Some(node.id());
        }
        if target.is_some() {
            return target;
        }

        for node in &self.nodes {
            if !self.is_attached(node.id())
                || node.element_name() != Some("a")
                || node.attribute("name") != Some(fragment)
            {
                continue;
            }
            if target.is_some() {
                return None;
            }
            target = Some(node.id());
        }
        target
    }

    /// Resolve a bounded text fragment against the first complete visible
    /// layout run in document order. Ranges stay inside one run so matching
    /// never invents cross-node whitespace or browser range semantics.
    pub(crate) fn text_fragment_target(
        &self,
        layout: &NativeLayoutSnapshot,
        terms: &TextFragmentTerms,
    ) -> Option<NativeNodeId> {
        if terms.start.is_empty() {
            return None;
        }
        layout
            .text_runs
            .iter()
            .filter(|run| !run.truncated)
            .find_map(|run| {
                Self::text_fragment_run_matches(&run.text, terms).then_some(run.node_id)
            })
    }

    pub(super) fn text_fragment_run_matches(text: &str, terms: &TextFragmentTerms) -> bool {
        let mut start_search = 0;
        while start_search < text.len() {
            let Some(relative_start) = text[start_search..].find(&terms.start) else {
                return false;
            };
            let start_offset = start_search.saturating_add(relative_start);
            let start_end = start_offset.saturating_add(terms.start.len());
            let prefix_matches = terms.prefix.as_ref().is_none_or(|prefix| {
                start_offset
                    .checked_sub(prefix.len())
                    .and_then(|prefix_start| text.get(prefix_start..start_offset))
                    == Some(prefix.as_str())
            });
            if prefix_matches {
                let (match_end, range_matches) = if let Some(end) = terms.end.as_deref() {
                    match text
                        .get(start_end..)
                        .and_then(|remaining| remaining.find(end))
                        .map(|relative_end| start_end.saturating_add(relative_end))
                    {
                        Some(end_offset) => (end_offset.saturating_add(end.len()), true),
                        None => (start_end, false),
                    }
                } else {
                    (start_end, true)
                };
                let suffix_matches = terms.suffix.as_ref().is_none_or(|suffix| {
                    text.get(match_end..)
                        .is_some_and(|remaining| remaining.starts_with(suffix))
                });
                if range_matches && suffix_matches {
                    return true;
                }
            }
            start_search = start_end;
        }
        false
    }

    pub(super) fn semantic_role(&self, id: NativeNodeId) -> Option<&'static str> {
        let node = self.node(id)?;
        if node.element_name() == Some("area")
            && (!self.is_hyperlink_element(id) || self.image_map_for_area(id).is_none())
        {
            return None;
        }
        if let Some(role) = node
            .attribute("role")
            .and_then(|role| supported_role(role.trim()))
        {
            return Some(role);
        }
        let name = node.element_name()?;
        match name {
            "button" => Some("button"),
            "a" if node.attribute("href").is_some() => Some("link"),
            "area" if self.is_hyperlink_element(id) => Some("link"),
            "textarea" => Some("textbox"),
            "select" => Some("combobox"),
            "option" => Some("option"),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Some("heading"),
            "img" => Some("img"),
            "input" => match node
                .attribute("type")
                .unwrap_or("text")
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "button" | "submit" | "reset" | "image" => Some("button"),
                "checkbox" => Some("checkbox"),
                "radio" => Some("radio"),
                "file" => Some("file"),
                "text" | "email" | "password" | "search" | "tel" | "url" => Some("textbox"),
                _ => None,
            },
            _ => None,
        }
    }

    pub(super) fn accessible_name(&self, id: NativeNodeId, role: &str) -> (String, bool) {
        let Some(node) = self.node(id) else {
            return (String::new(), false);
        };
        if let Some(value) = node
            .attribute("aria-label")
            .filter(|value| !value.is_empty())
        {
            return collapse_text(value, MAX_LOCATOR_BYTES);
        }
        if let Some(value) = node.attribute("aria-labelledby") {
            let mut labelled = String::new();
            for label_id in value.split_ascii_whitespace() {
                if let Some(label) = self.find_element_by_id(label_id) {
                    self.collect_raw_text(label, &mut labelled);
                    labelled.push(' ');
                }
            }
            let collapsed = collapse_text(&labelled, MAX_LOCATOR_BYTES);
            if !collapsed.0.is_empty() {
                return collapsed;
            }
        }
        if (role == "img" || (role == "link" && node.element_name() == Some("area")))
            && let Some(value) = node.attribute("alt")
        {
            return collapse_text(value, MAX_LOCATOR_BYTES);
        }
        if let Some(value) = node.attribute("id")
            && let Some(label) = self.find_label_for(value)
        {
            let mut text = String::new();
            self.collect_raw_text(label, &mut text);
            let collapsed = collapse_text(&text, MAX_LOCATOR_BYTES);
            if !collapsed.0.is_empty() {
                return collapsed;
            }
        }
        if let Some(label) = self.find_ancestor_label(id) {
            let mut text = String::new();
            self.collect_raw_text(label, &mut text);
            let collapsed = collapse_text(&text, MAX_LOCATOR_BYTES);
            if !collapsed.0.is_empty() {
                return collapsed;
            }
        }
        if role == "textbox"
            && let Some(value) = node
                .attribute("placeholder")
                .filter(|value| !value.is_empty())
        {
            return collapse_text(value, MAX_LOCATOR_BYTES);
        }
        let mut text = String::new();
        self.collect_raw_text(id, &mut text);
        collapse_text(&text, MAX_LOCATOR_BYTES)
    }

    pub(super) fn element_text(
        &self,
        id: NativeNodeId,
        max_bytes: usize,
    ) -> Option<(String, bool)> {
        self.node(id)?;
        let mut text = String::new();
        self.collect_raw_text(id, &mut text);
        Some(collapse_text(&text, max_bytes))
    }

    pub(super) fn current_value(&self, id: NativeNodeId) -> Option<String> {
        let node = self.node(id)?;
        match node.element_name()? {
            "input" => node.state.value.clone(),
            "textarea" => node.state.value.clone().or_else(|| {
                let mut text = String::new();
                self.collect_raw_text(id, &mut text);
                Some(text)
            }),
            "select" => {
                let option_ids = self.select_option_ids(id);
                let option_id = option_ids
                    .iter()
                    .copied()
                    .find(|option_id| {
                        self.node(*option_id)
                            .is_some_and(|option| option.state.selected)
                    })
                    .or_else(|| option_ids.first().copied());
                let Some(option_id) = option_id else {
                    return Some(String::new());
                };
                let option = self.node(option_id)?;
                option.attribute("value").map(str::to_owned).or_else(|| {
                    self.element_text(option_id, MAX_LOCATOR_BYTES)
                        .map(|(value, _)| value)
                })
            }
            _ => None,
        }
    }

    pub(super) fn is_disabled(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return true;
        };
        if node.attribute("disabled").is_some()
            || node
                .attribute("aria-disabled")
                .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        {
            return true;
        }
        let mut parent = node.parent();
        while let Some(parent_id) = parent {
            let Some(parent_node) = self.node(parent_id) else {
                break;
            };
            if parent_node.element_name() == Some("fieldset")
                && parent_node.attribute("disabled").is_some()
            {
                let first_legend = parent_node.children().iter().find_map(|child_id| {
                    self.node(*child_id)
                        .is_some_and(|child| child.element_name() == Some("legend"))
                        .then_some(*child_id)
                });
                if !first_legend.is_some_and(|legend_id| {
                    id == legend_id || self.is_descendant_of(id, legend_id)
                }) {
                    return true;
                }
            }
            if parent_node.element_name() == Some("select")
                && parent_node.attribute("disabled").is_some()
            {
                return true;
            }
            parent = parent_node.parent();
        }
        false
    }

    pub(super) fn is_actually_disabled_native_focus_target(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return true;
        };
        if node.state.form_associated_disabled == Some(true) {
            return true;
        }
        let tag_name = node.element_name().unwrap_or_default();
        let supports_disabled = matches!(
            tag_name,
            "button" | "fieldset" | "input" | "optgroup" | "option" | "select" | "textarea"
        );
        if supports_disabled && node.attribute("disabled").is_some() {
            return true;
        }

        if matches!(
            tag_name,
            "button" | "fieldset" | "input" | "select" | "textarea"
        ) {
            let mut parent = node.parent();
            while let Some(parent_id) = parent {
                let Some(parent_node) = self.node(parent_id) else {
                    break;
                };
                if parent_node.element_name() == Some("fieldset")
                    && parent_node.attribute("disabled").is_some()
                {
                    let first_legend = parent_node.children().iter().find_map(|child_id| {
                        self.node(*child_id)
                            .is_some_and(|child| child.element_name() == Some("legend"))
                            .then_some(*child_id)
                    });
                    if !first_legend.is_some_and(|legend_id| {
                        id == legend_id || self.is_descendant_of(id, legend_id)
                    }) {
                        return true;
                    }
                }
                parent = parent_node.parent();
            }
        }

        if tag_name == "option" {
            let mut parent = node.parent();
            while let Some(parent_id) = parent {
                let Some(parent_node) = self.node(parent_id) else {
                    break;
                };
                if matches!(parent_node.element_name(), Some("optgroup" | "select"))
                    && parent_node.attribute("disabled").is_some()
                {
                    return true;
                }
                if parent_node.element_name() == Some("select") {
                    break;
                }
                parent = parent_node.parent();
            }
        }
        false
    }

    pub(super) fn can_focus_explicit_tabindex(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        if !self.is_attached(id)
            || self.is_hidden(id)
            || self.is_inert(id)
            || self.is_actually_disabled_native_focus_target(id)
            || node.element_name() == Some("area")
        {
            return false;
        }
        if node.element_name() == Some("input")
            && node
                .attribute("type")
                .is_some_and(|input_type| input_type.eq_ignore_ascii_case("hidden"))
        {
            return false;
        }
        true
    }

    pub(super) fn is_option_disabled(&self, id: NativeNodeId) -> bool {
        let Some(option) = self.node(id) else {
            return true;
        };
        if option.element_name() != Some("option") || option.attribute("disabled").is_some() {
            return true;
        }
        let mut parent = option.parent();
        while let Some(parent_id) = parent {
            let Some(parent_node) = self.node(parent_id) else {
                break;
            };
            match parent_node.element_name() {
                Some("optgroup") if parent_node.attribute("disabled").is_some() => return true,
                Some("select") => break,
                _ => parent = parent_node.parent(),
            }
        }
        false
    }

    pub(super) fn select_display_size_is_one(&self, id: NativeNodeId) -> bool {
        let Some(size) = self.node(id).and_then(|select| select.attribute("size")) else {
            return true;
        };
        let digits = size
            .trim_start_matches(|character: char| character.is_ascii_whitespace())
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() {
            return true;
        }
        digits.trim_start_matches('0') == "1"
    }

    pub(super) fn is_hidden(&self, id: NativeNodeId) -> bool {
        let mut current = Some(id);
        while let Some(current_id) = current {
            let Some(node) = self.node(current_id) else {
                break;
            };
            if let Some(attributes) = node.attributes()
                && has_hidden_signal(attributes)
            {
                return true;
            }
            if self.computed_style_for_layout(current_id).hidden() {
                return true;
            }
            if let Some(parent) = node.parent() {
                if self
                    .node(parent)
                    .is_some_and(|parent_node| parent_node.shadow_root.is_some())
                    && !self.is_assigned_shadow_child(parent, current_id)
                {
                    return true;
                }
                current = Some(parent);
            } else {
                current = node.shadow_host;
            }
        }
        false
    }

    /// Capture effective hidden state in document order so snapshot walks do
    /// not repeat an ancestor scan for every descendant element. Nodes omitted
    /// by slot composition remain hidden in the raw JavaScript snapshot.
    pub(super) fn effective_hidden_by_index(&self) -> HashMap<u32, bool> {
        let mut hidden_by_index = HashMap::with_capacity(self.nodes.len());
        let mut pending = vec![(self.root, false)];
        while let Some((id, inherited_hidden)) = pending.pop() {
            let Some(node) = self.node(id) else {
                continue;
            };
            let own_hidden = node.attributes().is_some_and(has_hidden_signal)
                || node
                    .element_name()
                    .is_some_and(|_| self.computed_style_for_layout(id).hidden());
            let hidden = inherited_hidden || own_hidden;
            hidden_by_index.insert(id.index(), hidden);
            pending.extend(
                self.composed_children(id)
                    .into_iter()
                    .rev()
                    .map(|child| (child, hidden)),
            );
            if let Some(template_content) = node.template_content {
                // Template descendants are attached for DOM snapshots, but
                // their parent chain does not inherit from the template host.
                pending.push((template_content, false));
            }
        }

        // The composed walk intentionally omits light children that are not
        // assigned to a slot. They remain in the raw DOM snapshot for script
        // queries, but must not appear as visible or accessible content.
        let mut attached_by_index = vec![false; self.nodes.len()];
        let mut raw_pending = vec![self.root];
        while let Some(id) = raw_pending.pop() {
            if id.generation != self.generation {
                continue;
            }
            let Some(attached) = attached_by_index.get_mut(id.index as usize) else {
                continue;
            };
            if *attached {
                continue;
            }
            *attached = true;
            let Some(node) = self.node(id) else {
                continue;
            };
            raw_pending.extend(node.children().iter().copied());
            raw_pending.extend(node.shadow_root);
            raw_pending.extend(node.template_content);
        }
        for (index, attached) in attached_by_index.into_iter().enumerate() {
            let index = index as u32;
            if attached && !hidden_by_index.contains_key(&index) {
                hidden_by_index.insert(index, true);
            }
        }
        hidden_by_index
    }

    pub(super) fn is_inert(&self, id: NativeNodeId) -> bool {
        let mut current = Some(id);
        while let Some(current_id) = current {
            let Some(node) = self.node(current_id) else {
                break;
            };
            if node.attribute_in_namespace(None, "inert").is_some() {
                return true;
            }
            current = node.parent().or(node.shadow_host);
        }
        false
    }

    pub(super) fn is_read_only(&self, id: NativeNodeId) -> bool {
        self.node(id).is_some_and(|node| {
            node.attribute("readonly").is_some()
                || node
                    .attribute("aria-readonly")
                    .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        })
    }

    pub(super) fn is_required(&self, id: NativeNodeId) -> bool {
        self.node(id)
            .is_some_and(|node| node.attribute("required").is_some())
    }

    pub(super) fn find_element_by_id(&self, value: &str) -> Option<NativeNodeId> {
        self.nodes.iter().find_map(|node| {
            (self.is_attached(node.id())
                && node.element_name().is_some()
                && node.attribute("id") == Some(value))
            .then_some(node.id())
        })
    }

    pub(super) fn find_label_for(&self, value: &str) -> Option<NativeNodeId> {
        self.nodes.iter().find_map(|node| {
            (self.is_attached(node.id())
                && node.element_name() == Some("label")
                && node.attribute("for") == Some(value))
            .then_some(node.id())
        })
    }

    pub(super) fn find_ancestor_label(&self, id: NativeNodeId) -> Option<NativeNodeId> {
        let mut parent = self.node(id)?.parent();
        while let Some(parent_id) = parent {
            let parent_node = self.node(parent_id)?;
            if parent_node.element_name() == Some("label") {
                return Some(parent_id);
            }
            parent = parent_node.parent();
        }
        None
    }

    pub(super) fn find_element(&self, id: NativeNodeId, wanted: &str) -> Option<NativeNodeId> {
        let node = self.node(id)?;
        if matches!(node.kind(), NativeNodeKind::Element { name, .. } if name == wanted) {
            return Some(id);
        }
        node.children()
            .iter()
            .find_map(|child| self.find_element(*child, wanted))
    }

    pub(super) fn collect_raw_text(&self, id: NativeNodeId, output: &mut String) {
        let Some(node) = self.node(id) else {
            return;
        };
        match node.kind() {
            NativeNodeKind::Text(value) => output.push_str(value),
            NativeNodeKind::Document
            | NativeNodeKind::DocumentFragment
            | NativeNodeKind::Element { .. } => {
                for child in node.children() {
                    self.collect_raw_text(*child, output);
                }
            }
            NativeNodeKind::DocumentType { .. } | NativeNodeKind::Comment(_) => {}
        }
    }

    pub(super) fn collect_visible(
        &self,
        id: NativeNodeId,
        hidden_parent: bool,
        hidden_by_index: &HashMap<u32, bool>,
        output: &mut String,
        truncated: &mut bool,
        max_bytes: usize,
    ) {
        if *truncated {
            return;
        }
        let Some(node) = self.node(id) else {
            return;
        };
        match node.kind() {
            NativeNodeKind::Text(value) if !hidden_parent => {
                append_collapsed_text(output, value, max_bytes, truncated);
            }
            NativeNodeKind::Document | NativeNodeKind::DocumentFragment => {
                for child in self.composed_children(id) {
                    self.collect_visible(
                        child,
                        hidden_parent,
                        hidden_by_index,
                        output,
                        truncated,
                        max_bytes,
                    );
                }
            }
            NativeNodeKind::Element { name, .. } => {
                if node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                    && matches!(name.as_str(), "iframe" | "frame")
                {
                    return;
                }
                let hidden = hidden_parent
                    || matches!(
                        name.as_str(),
                        "head" | "script" | "style" | "template" | "title"
                    )
                    || hidden_by_index.get(&id.index()).copied().unwrap_or(false);
                for child in self.composed_children(id) {
                    self.collect_visible(
                        child,
                        hidden,
                        hidden_by_index,
                        output,
                        truncated,
                        max_bytes,
                    );
                }
            }
            NativeNodeKind::Text(_) => {}
            NativeNodeKind::DocumentType { .. } | NativeNodeKind::Comment(_) => {}
        }
    }
}

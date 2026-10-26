use super::*;

pub(crate) type ExternalStylesheetLinkWithNonce =
    (u32, String, Option<String>, Option<String>, Option<String>);
pub(crate) type MediaProperties = (u8, u8, Option<u64>, String, Option<u16>);

impl NativeDocument {
    pub(crate) fn external_stylesheet_states(
        &self,
    ) -> Vec<(u32, String, Option<String>, Option<String>)> {
        self.external_stylesheet_states
            .iter()
            .map(|(node_index, state)| {
                (
                    *node_index,
                    state.href.clone(),
                    state.stylesheet_url.clone(),
                    state.body.clone(),
                )
            })
            .collect()
    }

    pub(crate) fn external_image_preload_states(&self) -> Vec<(u32, String)> {
        self.external_image_preload_states
            .iter()
            .map(|(node_index, href)| (*node_index, href.clone()))
            .collect()
    }

    pub(crate) fn cache_preloaded_image_resource(
        &mut self,
        url: String,
        image: NativeImage,
    ) -> Result<bool, NativeEngineError> {
        validate_url_text("preloaded image URL", &url)?;
        let image_bytes = image.decoded_bytes().ok_or_else(|| {
            NativeEngineError::invalid(
                "preloaded image resource",
                "decoded pixel size is not representable",
            )
        })?;
        if image_bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES {
            return Err(NativeEngineError::limit(
                "preloaded image resource bytes",
                MAX_NATIVE_IMAGE_TRANSFER_BYTES,
                image_bytes,
            ));
        }
        let retained_bytes = self
            .preloaded_image_resources
            .values()
            .try_fold(0usize, |total, retained| {
                total.checked_add(retained.decoded_bytes()?)
            })
            .ok_or_else(|| {
                NativeEngineError::limit(
                    "document preload image bytes",
                    MAX_NATIVE_DOCUMENT_PRELOAD_IMAGE_BYTES,
                    usize::MAX,
                )
            })?;
        let replaced_bytes = self
            .preloaded_image_resources
            .get(&url)
            .and_then(NativeImage::decoded_bytes)
            .unwrap_or_default();
        let next_bytes = retained_bytes
            .saturating_sub(replaced_bytes)
            .checked_add(image_bytes)
            .ok_or_else(|| {
                NativeEngineError::limit(
                    "document preload image bytes",
                    MAX_NATIVE_DOCUMENT_PRELOAD_IMAGE_BYTES,
                    usize::MAX,
                )
            })?;
        if (!self.preloaded_image_resources.contains_key(&url)
            && self.preloaded_image_resources.len() >= MAX_NATIVE_DOCUMENT_PRELOAD_IMAGES)
            || next_bytes > MAX_NATIVE_DOCUMENT_PRELOAD_IMAGE_BYTES
        {
            return Ok(false);
        }
        self.preloaded_image_resources.insert(url, image);
        self.layout_style_cache.invalidate_script_snapshot();
        Ok(true)
    }

    pub(crate) fn take_preloaded_image_resource(&mut self, url: &str) -> Option<NativeImage> {
        let removed = self.preloaded_image_resources.remove(url);
        if removed.is_some() {
            self.layout_style_cache.invalidate_script_snapshot();
        }
        removed
    }

    pub(crate) fn has_preloaded_image_resource(&self, url: &str) -> bool {
        self.preloaded_image_resources.contains_key(url)
    }

    pub(crate) fn rebuild_external_stylesheet(
        &mut self,
        document_url: &str,
    ) -> Result<(), NativeEngineError> {
        let external_sources = self
            .external_stylesheet_states
            .values()
            .filter_map(|state| {
                state.body.as_ref().map(|body| {
                    let stylesheet_url = state.stylesheet_url.as_deref().unwrap_or(&state.href);
                    absolutize_stylesheet_urls(body, document_url, stylesheet_url)
                })
            })
            .collect::<Vec<_>>();
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
        style_sources.extend(external_sources);
        let style_sources = super::parser::with_html_user_agent_stylesheet(style_sources);
        let mut diagnostics = NativeDiagnosticSink::default();
        let stylesheet =
            NativeStylesheet::from_sources_with_diagnostics(style_sources, &mut diagnostics)?;
        let mut background_image_sources = stylesheet.background_image_sources().clone();
        for node in &self.nodes {
            if !node.inline_style_allowed() {
                continue;
            }
            let Some(inline_style) = node.attribute("style") else {
                continue;
            };
            collect_background_image_sources(inline_style, &mut background_image_sources);
            crate::native_engine::css::collect_declaration_diagnostics(
                inline_style,
                NativeDiagnosticSource::InlineStyle {
                    node_index: node.id().index(),
                },
                0,
                &mut diagnostics,
            );
        }
        let (diagnostics, diagnostics_truncated) = diagnostics.finish();
        self.stylesheet = stylesheet;
        self.font_resources.clear();
        self.font_book = NativeFontBook::system();
        self.background_image_sources = background_image_sources;
        self.diagnostics = diagnostics;
        self.diagnostics_truncated = diagnostics_truncated;
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(crate) fn external_stylesheet_links(
        &self,
    ) -> Vec<(u32, String, Option<String>, Option<String>)> {
        self.nodes
            .iter()
            .filter_map(|node| {
                self.node(node.id())?;
                (node.element_name() == Some("link")
                    && node.attribute("rel").is_some_and(|rel| {
                        rel.split_ascii_whitespace()
                            .any(|token| token.eq_ignore_ascii_case("stylesheet"))
                    }))
                .then(|| {
                    node.attribute("href").map(|href| {
                        (
                            node.id().index(),
                            href.to_owned(),
                            node.attribute("integrity").map(str::to_owned),
                            node.attribute("crossorigin").map(str::to_owned),
                        )
                    })
                })
                .flatten()
            })
            .collect()
    }

    pub(crate) fn external_image_preload_links(&self) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter_map(|node| {
                let node_id = node.id();
                self.node(node_id)?;
                (node.element_name() == Some("link")
                    && node.state.namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI)
                    && self.is_attached(node_id)
                    && node.attribute("rel").is_some_and(|rel| {
                        rel.split_ascii_whitespace()
                            .any(|token| token.eq_ignore_ascii_case("preload"))
                    })
                    && node
                        .attribute("as")
                        .is_some_and(|destination| destination.eq_ignore_ascii_case("image"))
                    && node.attribute("media").is_none()
                    && node.attribute("type").is_none()
                    && node.attribute("crossorigin").is_none()
                    && node.attribute("integrity").is_none()
                    && node.attribute("imagesrcset").is_none()
                    && node.attribute("imagesizes").is_none())
                .then(|| {
                    node.attribute("href")
                        .filter(|href| !href.is_empty())
                        .map(|href| (node_id.index(), href.to_owned()))
                })
                .flatten()
            })
            .collect()
    }

    pub(crate) fn external_stylesheet_links_with_nonce(
        &self,
    ) -> Vec<ExternalStylesheetLinkWithNonce> {
        self.external_stylesheet_links()
            .into_iter()
            .map(|(node_index, href, integrity, crossorigin)| {
                let nonce = self
                    .nodes
                    .get(node_index as usize)
                    .and_then(|node| node.attribute("nonce"))
                    .map(str::to_owned);
                (node_index, href, integrity, crossorigin, nonce)
            })
            .collect()
    }

    pub(crate) fn external_image_links(&self, viewport: Viewport) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter_map(|node| {
                self.node(node.id())?;
                let source = self.selected_image_source(node.id(), viewport)?;
                if source.is_empty()
                    || source
                        .get(..5)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
                {
                    return None;
                }
                is_image_resource_node_in(&self.nodes, node.id().index() as usize)
                    .then_some((node.id().index(), source))
            })
            .collect()
    }

    /// Return media elements whose selected source needs native resource
    /// admission. Source children are considered in document order after the
    /// element's own src attribute, matching the bounded HTML media source
    /// selection model.
    pub(crate) fn external_media_links(&self) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter_map(|node| {
                self.node(node.id())?;
                let source = self.selected_media_source(node.id())?;
                (node.element_name() == Some("audio") || node.element_name() == Some("video"))
                    .then_some((node.id().index(), source))
            })
            .filter(|(_, source)| !source.is_empty())
            .collect()
    }

    pub(crate) fn external_video_poster_links(&self) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter_map(|node| {
                let node_id = node.id();
                let source = node.attribute("poster")?;
                (node.element_name() == Some("video")
                    && node.state.namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI)
                    && self.is_attached(node_id)
                    && !source.trim().is_empty()
                    && !source
                        .trim()
                        .get(..5)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:")))
                .then(|| (node_id.index(), source.to_owned()))
            })
            .collect()
    }

    pub(super) fn selected_media_source(&self, node_id: NativeNodeId) -> Option<String> {
        let node = self.node(node_id)?;
        if node.element_name() != Some("audio") && node.element_name() != Some("video") {
            return None;
        }
        if let Some(source) = node.attribute("src").filter(|source| !source.is_empty()) {
            return Some(source.to_owned());
        }
        for child_id in node.children() {
            let child = self.node(*child_id)?;
            if child.element_name() != Some("source")
                || !media_type_is_supported(child.attribute("type"))
            {
                continue;
            }
            if let Some(source) = child.attribute("src").filter(|source| !source.is_empty()) {
                return Some(source.to_owned());
            }
        }
        None
    }

    pub(crate) fn media_current_src(&self, node_id: NativeNodeId) -> String {
        self.media_loads
            .get(&node_id.index())
            .cloned()
            .or_else(|| self.media_errors.get(&node_id.index()).cloned())
            .or_else(|| self.selected_media_source(node_id))
            .unwrap_or_default()
    }

    /// Return the bounded HTMLMediaElement resource state:
    /// (ready_state, network_state, duration_millis, current_src, error).
    pub(crate) fn media_properties(&self, node_id: NativeNodeId) -> Option<MediaProperties> {
        let node = self.node(node_id)?;
        if node.element_name() != Some("audio") && node.element_name() != Some("video") {
            return None;
        }
        let source = self.media_current_src(node_id);
        if source.is_empty() {
            return Some((0, 0, None, String::new(), None));
        }
        if let Some(resource) = self.media_resources.get(&node_id.index())
            && resource.source == source
        {
            return Some((1, 1, resource.duration_millis, source, None));
        }
        if self
            .media_errors
            .get(&node_id.index())
            .is_some_and(|loaded_source| loaded_source == &source)
        {
            return Some((0, 3, None, source, Some(4)));
        }
        Some((0, 2, None, source, None))
    }

    pub(crate) fn mark_media_load(
        &mut self,
        node_index: u32,
        source: String,
    ) -> Result<(), NativeEngineError> {
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if (node.element_name() != Some("audio") && node.element_name() != Some("video"))
            || self.selected_media_source(node_id).as_deref() != Some(source.as_str())
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "media load state does not match its media element".into(),
            });
        }
        let removed_resource = self.media_resources.remove(&node_index).is_some();
        let removed_error = self.media_errors.remove(&node_index).is_some();
        let previous_source = self.media_loads.insert(node_index, source.clone());
        let changed = removed_resource
            || removed_error
            || previous_source.as_deref() != Some(source.as_str());
        if changed {
            self.layout_style_cache.invalidate_script_snapshot();
        }
        Ok(())
    }

    pub(crate) fn refresh_media_loads(&mut self) {
        let mut changed = false;
        let retained_loads = self
            .media_loads
            .iter()
            .filter_map(|(node_index, source)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                let node = self.node(node_id)?;
                ((node.element_name() == Some("audio") || node.element_name() == Some("video"))
                    && self.selected_media_source(node_id).as_deref() == Some(source.as_str()))
                .then(|| (*node_index, source.clone()))
            })
            .collect();
        if self.media_loads != retained_loads {
            self.media_loads = retained_loads;
            changed = true;
        }
        let retained_errors = self
            .media_errors
            .iter()
            .filter_map(|(node_index, source)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                let node = self.node(node_id)?;
                ((node.element_name() == Some("audio") || node.element_name() == Some("video"))
                    && self.selected_media_source(node_id).as_deref() == Some(source.as_str()))
                .then(|| (*node_index, source.clone()))
            })
            .collect();
        if self.media_errors != retained_errors {
            self.media_errors = retained_errors;
            changed = true;
        }
        self.media_resources.retain(|node_index, resource| {
            let retain = self
                .media_loads
                .get(node_index)
                .is_some_and(|source| source == &resource.source);
            changed |= !retain;
            retain
        });
        if changed {
            self.layout_style_cache.invalidate_script_snapshot();
        }
    }

    pub(crate) fn has_media_resource_for_node(&self, node_id: NativeNodeId) -> bool {
        self.media_resources
            .get(&node_id.index())
            .is_some_and(|resource| {
                self.media_loads.get(&node_id.index()) == Some(&resource.source)
            })
    }

    pub(crate) fn media_load_attempted_for_node(&self, node_id: NativeNodeId) -> bool {
        self.media_loads
            .get(&node_id.index())
            .is_some_and(|source| {
                self.selected_media_source(node_id).as_deref() == Some(source.as_str())
            })
    }

    pub(crate) fn set_media_resource(
        &mut self,
        node_index: u32,
        source: String,
        metadata: NativeMediaMetadata,
    ) -> Result<(), NativeEngineError> {
        if source.is_empty()
            || source.len() > MAX_ATTRIBUTE_BYTES
            || source.bytes().any(|byte| byte.is_ascii_control())
            || metadata.content_type.len() > MAX_ATTRIBUTE_BYTES
            || !media_type_is_supported(Some(&metadata.content_type))
            || metadata.byte_length == 0
            || metadata.byte_length > MAX_NATIVE_MEDIA_BYTES
        {
            return Err(NativeEngineError::invalid(
                "native media resource",
                "contains an invalid bounded media resource",
            ));
        }
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if (node.element_name() != Some("audio") && node.element_name() != Some("video"))
            || self.media_loads.get(&node_index) != Some(&source)
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "media resource does not match its media element".into(),
            });
        }
        self.media_errors.remove(&node_index);
        let retained_video_bytes = self
            .media_resources
            .iter()
            .filter(|(existing_index, resource)| {
                **existing_index != node_index && resource.source != source
            })
            .flat_map(|(_, resource)| resource.video_frames.iter())
            .fold(0usize, |total, frame| {
                total.saturating_add(frame.pixels.len())
            });
        let retained_video_frames = self
            .media_resources
            .iter()
            .filter(|(existing_index, resource)| {
                **existing_index != node_index && resource.source != source
            })
            .map(|(_, resource)| resource.video_frames.len())
            .fold(0usize, usize::saturating_add);
        let incoming_video_bytes = metadata.video_frames.iter().fold(0usize, |total, frame| {
            total.saturating_add(frame.pixels.len())
        });
        let mut video_frames = metadata.video_frames;
        let mut duration_millis = metadata.duration_millis;
        if retained_video_bytes.saturating_add(incoming_video_bytes)
            > MAX_NATIVE_DOCUMENT_VIDEO_BYTES
            || retained_video_frames.saturating_add(video_frames.len())
                > MAX_NATIVE_DOCUMENT_VIDEO_FRAMES
        {
            video_frames.clear();
            duration_millis = None;
        }
        self.media_resources.insert(
            node_index,
            NativeMediaResource {
                source,
                content_type: metadata.content_type,
                byte_length: metadata.byte_length,
                duration_millis,
                video_frames,
                current_time_millis: 0,
                is_playing: false,
                playback_rate_millionths: 1_000_000,
                playback_started_at: None,
            },
        );
        self.layout_style_cache.invalidate_script_snapshot();
        Ok(())
    }

    pub(crate) fn decoded_video_frame_for_node(
        &self,
        node_id: NativeNodeId,
    ) -> Option<&NativeVideoFrame> {
        let resource = self.media_resources.get(&node_id.index())?;
        if resource.source != self.media_current_src(node_id) {
            return None;
        }
        let playback_elapsed_ms = resource
            .playback_started_at
            .filter(|_| resource.is_playing)
            .map(|started_at| {
                started_at.elapsed().as_millis() as f64
                    * (resource.playback_rate_millionths as f64 / 1_000_000.0)
            })
            .unwrap_or_default();
        let current_time_millis = (resource.current_time_millis as f64 + playback_elapsed_ms)
            .max(0.0)
            .min(resource.duration_millis.unwrap_or(u64::MAX) as f64)
            as u64;
        resource
            .video_frames
            .iter()
            .take_while(|frame| frame.timestamp_millis <= current_time_millis)
            .last()
            .or_else(|| resource.video_frames.first())
    }

    pub(crate) fn set_media_frame_time(
        &mut self,
        node_index: u32,
        time_millis: u64,
        is_playing: bool,
        playback_rate: f64,
    ) -> Result<(), NativeEngineError> {
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("video") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "decoded video frame time requires a video element".into(),
            });
        }
        let resource = self.media_resources.get_mut(&node_index).ok_or(
            NativeEngineError::TargetNotActionable {
                reason: "video frame time requires a loaded media resource".into(),
            },
        )?;
        if resource.video_frames.is_empty() || !playback_rate.is_finite() || playback_rate == 0.0 {
            return Ok(());
        }
        resource.current_time_millis =
            time_millis.min(resource.duration_millis.unwrap_or(u64::MAX));
        resource.is_playing = is_playing;
        let mut playback_rate_millionths = (playback_rate * 1_000_000.0).round() as i64;
        if playback_rate > 0.0 {
            playback_rate_millionths = playback_rate_millionths.max(1);
        } else {
            playback_rate_millionths = playback_rate_millionths.min(-1);
        }
        resource.playback_rate_millionths = playback_rate_millionths;
        resource.playback_started_at = is_playing.then(std::time::Instant::now);
        Ok(())
    }

    pub(crate) fn set_media_error(
        &mut self,
        node_index: u32,
        source: String,
    ) -> Result<(), NativeEngineError> {
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if (node.element_name() != Some("audio") && node.element_name() != Some("video"))
            || self.media_loads.get(&node_index) != Some(&source)
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "media error does not match its media element".into(),
            });
        }
        self.media_resources.remove(&node_index);
        self.media_errors.insert(node_index, source);
        self.layout_style_cache.invalidate_script_snapshot();
        Ok(())
    }

    fn selected_image_source(&self, node_id: NativeNodeId, viewport: Viewport) -> Option<String> {
        let node = self.node(node_id)?;
        if self.is_svg_image_node(node_id) {
            return svg_image_href(node).map(str::to_owned);
        }
        if node.element_name() != Some("img") {
            return None;
        }
        let fallback = node.attribute("src").map(str::to_owned);
        if let Some((srcset, sizes)) = self.picture_image_source_set(node_id, viewport)
            && let Some(source) = select_image_srcset_source(&srcset, sizes.as_deref(), viewport)
        {
            return Some(source);
        }
        if let Some(srcset) = node.attribute("srcset")
            && let Some(source) =
                select_image_srcset_source(srcset, node.attribute("sizes"), viewport)
        {
            return Some(source);
        }
        fallback
    }

    pub(crate) fn is_svg_image_node(&self, node_id: NativeNodeId) -> bool {
        is_svg_image_node_in(&self.nodes, node_id.index() as usize)
    }

    pub(crate) fn svg_image_source_for_node(&self, node_id: NativeNodeId) -> Option<&str> {
        self.is_svg_image_node(node_id)
            .then(|| self.node(node_id).and_then(svg_image_href))
            .flatten()
    }

    fn picture_image_source_set(
        &self,
        node_id: NativeNodeId,
        viewport: Viewport,
    ) -> Option<(String, Option<String>)> {
        let node = self.node(node_id)?;
        let parent_id = node.parent()?;
        let parent = self.node(parent_id)?;
        if parent.element_name() != Some("picture") {
            return None;
        }
        for child_id in parent.children() {
            if *child_id == node_id {
                break;
            }
            let source = self.node(*child_id)?;
            if source.element_name() != Some("source")
                || !image_media_matches(source.attribute("media"), viewport)
                || !image_type_is_supported(source.attribute("type"))
            {
                continue;
            }
            let Some(srcset) = source.attribute("srcset") else {
                continue;
            };
            if parse_image_srcset(srcset).is_empty() {
                continue;
            }
            return Some((
                srcset.to_owned(),
                source.attribute("sizes").map(str::to_owned),
            ));
        }
        None
    }

    pub(crate) fn image_current_src(&self, node_id: NativeNodeId, viewport: Viewport) -> String {
        self.image_loads
            .get(&node_id.index())
            .cloned()
            .or_else(|| self.selected_image_source(node_id, viewport))
            .unwrap_or_default()
    }

    pub(crate) fn image_properties(
        &self,
        node_id: NativeNodeId,
        viewport: Viewport,
    ) -> Option<(bool, u32, u32)> {
        let node = self.node(node_id)?;
        if node.element_name() != Some("img") {
            return None;
        }
        let source = self.image_current_src(node_id, viewport);
        if source.is_empty() {
            return Some((true, 0, 0));
        }
        if let Some(image) = self.image_resource_for_node(node_id) {
            return Some((true, image.width, image.height));
        }
        if let Some(image) = decode_data_image(&source) {
            return Some((true, image.width, image.height));
        }
        Some((
            self.image_loads
                .get(&node_id.index())
                .is_some_and(|loaded_source| loaded_source == &source),
            0,
            0,
        ))
    }

    pub(crate) fn mark_image_load(
        &mut self,
        node_index: u32,
        source: String,
        viewport: Viewport,
    ) -> Result<(), NativeEngineError> {
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if (node.element_name() != Some("img") && !self.is_svg_image_node(node_id))
            || self.selected_image_source(node_id, viewport).as_deref() != Some(source.as_str())
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "image load state does not match its image element".into(),
            });
        }
        let previous_source = self.image_loads.insert(node_index, source.clone());
        if previous_source.as_deref() != Some(source.as_str()) {
            self.layout_style_cache.invalidate_script_snapshot();
        }
        Ok(())
    }

    pub(crate) fn refresh_image_loads(&mut self, viewport: Viewport) {
        let retained = self
            .image_loads
            .iter()
            .filter_map(|(node_index, source)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                let _node = self.node(node_id)?;
                (is_image_resource_node_in(&self.nodes, *node_index as usize)
                    && self.selected_image_source(node_id, viewport).as_deref()
                        == Some(source.as_str()))
                .then(|| (*node_index, source.clone()))
            })
            .collect();
        if self.image_loads != retained {
            self.image_loads = retained;
            self.layout_style_cache.invalidate_script_snapshot();
        }
    }

    pub(super) fn selected_video_poster_source(&self, node_id: NativeNodeId) -> Option<String> {
        let node = self.node(node_id)?;
        (node.element_name() == Some("video")
            && node.state.namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI)
            && self.is_attached(node_id))
        .then(|| {
            node.attribute("poster")
                .filter(|source| !source.trim().is_empty())
        })
        .flatten()
        .map(str::to_owned)
    }

    pub(crate) fn mark_video_poster_load(
        &mut self,
        node_index: u32,
        source: String,
    ) -> Result<bool, NativeEngineError> {
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        if self.selected_video_poster_source(node_id).as_deref() != Some(source.as_str()) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "poster load state does not match its attached video element".into(),
            });
        }
        if !self.video_poster_loads.contains_key(&node_index)
            && self.video_poster_loads.len() >= MAX_NATIVE_VIDEO_POSTERS
        {
            return Ok(false);
        }
        let removed_resource = self.video_poster_resources.remove(&node_index).is_some();
        self.video_poster_loads.insert(node_index, source);
        if removed_resource {
            self.invalidate_layout_styles();
        }
        Ok(true)
    }

    pub(crate) fn refresh_video_poster_loads(&mut self) {
        let retained = self
            .video_poster_loads
            .iter()
            .filter_map(|(node_index, source)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                (self.selected_video_poster_source(node_id).as_deref() == Some(source.as_str()))
                    .then(|| (*node_index, source.clone()))
            })
            .collect();
        self.video_poster_loads = retained;
        self.video_poster_resources.retain(|node_index, resource| {
            self.video_poster_loads.get(node_index) == Some(&resource.source)
        });
    }

    pub(crate) fn clear_video_poster_loads(&mut self) {
        let had_resources = !self.video_poster_resources.is_empty();
        self.video_poster_resources.clear();
        self.video_poster_loads.clear();
        if had_resources {
            self.invalidate_layout_styles();
        }
    }

    pub(crate) fn video_poster_resource_for_node(
        &self,
        node_id: NativeNodeId,
    ) -> Option<&NativeImage> {
        let source = self.selected_video_poster_source(node_id)?;
        let resource = self.video_poster_resources.get(&node_id.index())?;
        (self.video_poster_loads.get(&node_id.index()) == Some(&source)
            && resource.source == source)
            .then_some(&resource.image)
    }

    pub(crate) fn video_poster_load_attempted_for_node(&self, node_id: NativeNodeId) -> bool {
        self.video_poster_loads
            .get(&node_id.index())
            .is_some_and(|source| {
                self.selected_video_poster_source(node_id).as_deref() == Some(source.as_str())
            })
    }

    pub(crate) fn set_video_poster_resource(
        &mut self,
        node_index: u32,
        source: String,
        image: NativeImage,
    ) -> Result<(), NativeEngineError> {
        if source.is_empty()
            || source.len() > MAX_ATTRIBUTE_BYTES
            || source.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(NativeEngineError::invalid(
                "video poster source",
                "must be a bounded printable URL attribute",
            ));
        }
        let expected_bytes = usize::try_from(image.width)
            .ok()
            .and_then(|width| width.checked_mul(usize::try_from(image.height).ok()?))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| NativeEngineError::limit("video poster pixels", 0, usize::MAX))?;
        let decoded_bytes = image
            .decoded_bytes()
            .ok_or_else(|| NativeEngineError::limit("video poster frames", 0, usize::MAX))?;
        if image.width == 0
            || image.height == 0
            || expected_bytes != image.pixels.len()
            || decoded_bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES
            || expected_bytes / 4 > MAX_NATIVE_IMAGE_TRANSFER_PIXELS
        {
            return Err(NativeEngineError::limit(
                "video poster pixels",
                MAX_NATIVE_IMAGE_TRANSFER_BYTES,
                decoded_bytes,
            ));
        }
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        if self.video_poster_loads.get(&node_index) != Some(&source)
            || self.selected_video_poster_source(node_id).as_deref() != Some(source.as_str())
        {
            // An attribute mutation or detach can make an in-flight response
            // stale. It must not replace the current poster or fail navigation.
            return Ok(());
        }
        self.video_poster_resources
            .insert(node_index, NativeImageResource { source, image });
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(crate) fn background_image_source_for_node(&self, node_id: NativeNodeId) -> Option<&str> {
        let source_id = self.computed_style_for_layout(node_id).background_image()?;
        self.background_image_sources
            .get(&source_id)
            .map(String::as_str)
    }

    /// Return at most `limit` eligible background image candidates in document
    /// order. The eager resource loader uses its existing per-document cap,
    /// so do not cascade candidates that it would immediately discard.
    pub(crate) fn external_background_image_links(&self, limit: usize) -> Vec<(u32, String)> {
        let mut links = Vec::with_capacity(limit.min(self.nodes.len()));
        let mut rule_candidates = Vec::new();
        for node in &self.nodes {
            if links.len() >= limit {
                break;
            }
            let Some((node_index, source)) = (|| {
                node.element_name()?;
                let node_id = node.id();
                self.node(node_id)?;
                let has_inline_style =
                    node.inline_style_allowed() && node.attribute("style").is_some();
                if !has_inline_style
                    && !self
                        .stylesheet
                        .has_background_image_rule_for_node_with_candidates(
                            self,
                            node_id,
                            self.viewport,
                            &mut rule_candidates,
                        )
                {
                    return None;
                }
                let source = self.background_image_source_for_node(node_id)?;
                if source.is_empty()
                    || source
                        .get(..5)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
                {
                    return None;
                }
                Some((node_id.index(), source.to_owned()))
            })() else {
                continue;
            };
            links.push((node_index, source));
        }
        links
    }

    pub(crate) fn image_resource_for_node(&self, node_id: NativeNodeId) -> Option<&NativeImage> {
        let resource = self.image_resources.get(&node_id.index())?;
        let node = self.node(node_id)?;
        ((node.element_name() == Some("img") || self.is_svg_image_node(node_id))
            && self.image_loads.get(&node_id.index()) == Some(&resource.source))
        .then_some(&resource.image)
    }

    pub(crate) fn image_load_attempted_for_node(
        &self,
        node_id: NativeNodeId,
        viewport: Viewport,
    ) -> bool {
        self.image_loads
            .get(&node_id.index())
            .is_some_and(|source| {
                self.selected_image_source(node_id, viewport).as_deref() == Some(source.as_str())
            })
    }

    pub(crate) fn canvas_resource_for_node(
        &self,
        node_id: NativeNodeId,
    ) -> Option<&NativeCanvasResource> {
        let node = self.node(node_id)?;
        (node.element_name() == Some("canvas"))
            .then(|| self.canvas_resources.get(&node_id.index()))
            .flatten()
    }

    pub(crate) fn set_image_resource(
        &mut self,
        node_index: u32,
        source: String,
        image: NativeImage,
    ) -> Result<(), NativeEngineError> {
        if source.is_empty()
            || source.len() > MAX_ATTRIBUTE_BYTES
            || source.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(NativeEngineError::invalid(
                "external image source",
                "must be a bounded printable URL attribute",
            ));
        }
        let expected_bytes = usize::try_from(image.width)
            .ok()
            .and_then(|width| width.checked_mul(usize::try_from(image.height).ok()?))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| NativeEngineError::limit("external image pixels", 0, usize::MAX))?;
        let decoded_bytes = image
            .decoded_bytes()
            .ok_or_else(|| NativeEngineError::limit("external image frames", 0, usize::MAX))?;
        if image.width == 0
            || image.height == 0
            || expected_bytes != image.pixels.len()
            || decoded_bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES
            || expected_bytes / 4 > MAX_NATIVE_IMAGE_TRANSFER_PIXELS
        {
            return Err(NativeEngineError::limit(
                "external image pixels",
                MAX_NATIVE_IMAGE_TRANSFER_BYTES,
                decoded_bytes,
            ));
        }
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if (node.element_name() != Some("img") && !self.is_svg_image_node(node_id))
            || self.image_loads.get(&node_index) != Some(&source)
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "external image resource does not match its image element".into(),
            });
        }
        self.image_resources
            .insert(node_index, NativeImageResource { source, image });
        self.invalidate_layout_styles();
        Ok(())
    }

    pub(crate) fn background_image_resource_for_node(
        &self,
        node_id: NativeNodeId,
    ) -> Option<&NativeImage> {
        let resource = self.background_image_resources.get(&node_id.index())?;
        (self.background_image_source_for_node(node_id) == Some(resource.source.as_str()))
            .then_some(&resource.image)
    }

    pub(crate) fn set_background_image_resource(
        &mut self,
        node_index: u32,
        source: String,
        image: NativeImage,
    ) -> Result<(), NativeEngineError> {
        if source.is_empty()
            || source.len() > MAX_ATTRIBUTE_BYTES
            || source.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(NativeEngineError::invalid(
                "background image source",
                "must be a bounded printable URL",
            ));
        }
        let expected_bytes = usize::try_from(image.width)
            .ok()
            .and_then(|width| width.checked_mul(usize::try_from(image.height).ok()?))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| NativeEngineError::limit("background image pixels", 0, usize::MAX))?;
        let decoded_bytes = image
            .decoded_bytes()
            .ok_or_else(|| NativeEngineError::limit("background image frames", 0, usize::MAX))?;
        if image.width == 0
            || image.height == 0
            || expected_bytes != image.pixels.len()
            || decoded_bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES
            || expected_bytes / 4 > MAX_NATIVE_IMAGE_TRANSFER_PIXELS
        {
            return Err(NativeEngineError::limit(
                "background image pixels",
                MAX_NATIVE_IMAGE_TRANSFER_BYTES,
                decoded_bytes,
            ));
        }
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        if self.background_image_source_for_node(node_id) != Some(source.as_str()) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "background image resource does not match its computed style".into(),
            });
        }
        self.background_image_resources
            .insert(node_index, NativeImageResource { source, image });
        Ok(())
    }

    pub(crate) fn refresh_background_image_sources(&mut self) {
        let mut sources = self.stylesheet.background_image_sources().clone();
        let inline_sources = self
            .nodes
            .iter()
            .filter(|node| node.inline_style_allowed())
            .filter_map(|node| node.attribute("style").map(str::to_owned))
            .collect::<Vec<_>>();
        for inline_style in inline_sources {
            collect_background_image_sources(&inline_style, &mut sources);
        }
        self.background_image_sources = sources;
        let retained = self
            .background_image_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                (self.background_image_source_for_node(node_id) == Some(resource.source.as_str()))
                    .then(|| (*node_index, resource.clone()))
            })
            .collect();
        self.background_image_resources = retained;
    }
}

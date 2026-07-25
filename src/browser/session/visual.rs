use super::*;

impl BrowserSession {
    pub async fn screenshot_png(&self) -> BrowserResult<Vec<u8>> {
        let data = self.screenshot_base64().await?;
        Ok(STANDARD.decode(data.as_bytes())?)
    }

    /// Capture a PNG while preserving CDP's base64 payload for image APIs.
    pub async fn screenshot_base64(&self) -> BrowserResult<String> {
        self.policy.require(PolicyCapability::Screenshot)?;
        self.cdp
            .with_current_route(async { Ok(self.cdp.screenshot("png").await?) })
            .await
    }

    /// Capture exact opt-in visual evidence with explicit effective metadata.
    pub async fn capture_visual(
        &self,
        options: &VisualCaptureOptions,
    ) -> BrowserResult<VisualCapture> {
        self.policy.require(PolicyCapability::Screenshot)?;
        validate_visual_options(options)?;
        self.cdp
            .with_current_route(async {
                let metrics = self.cdp.get_layout_metrics().await?;
                let dpr = runtime_value(&self.cdp.evaluate("devicePixelRatio").await?)?
                    .as_f64()
                    .unwrap_or(1.0);
                let (_, selected_frame_id) = self.route_identity().await?;
                let selected_child_frame = {
                    let topology = self.topology.lock().await;
                    topology
                        .frames
                        .iter()
                        .find(|frame| frame.id == selected_frame_id)
                        .is_some_and(|frame| frame.parent_id.is_some())
                };
                if selected_child_frame {
                    return Err("exact visual capture of a selected child frame is not supported; select its page target or the main frame".into());
                }
                let mut clip = options.clip;
                if options.full_page {
                    clip = Some(visual_rect(&metrics["cssContentSize"])?);
                } else if let Some(target) = options.target.as_deref() {
                    let element = self.resolve_element(target).await?;
                    let model = match (element.node_id, element.backend_dom_node_id) {
                        (Some(node_id), _) => self.cdp.get_box_model(node_id).await?,
                        (_, Some(backend_id)) => {
                            self.cdp.get_box_model_for_backend(backend_id).await?
                        }
                        _ => return Err("visual target has no DOM node identity".into()),
                    };
                    let mut element_clip = visual_quad_rect(&model["model"]["border"])?;
                    let viewport = visual_viewport_rect(&metrics["cssVisualViewport"])?;
                    element_clip.x += viewport.x;
                    element_clip.y += viewport.y;
                    clip = Some(element_clip);
                } else if clip.is_none() && options.scale != 1.0 {
                    clip = Some(visual_viewport_rect(&metrics["cssVisualViewport"])?);
                }
                let viewport = visual_viewport_rect(&metrics["cssVisualViewport"])?;
                validate_effective_visual_clip(
                    Some(clip.unwrap_or(viewport)),
                    if clip.is_some() { options.scale } else { dpr },
                )?;
                let mut params = serde_json::json!({
                    "format": options.format.as_cdp(),
                    "optimizeForSpeed": true,
                    "captureBeyondViewport": options.full_page || clip.is_some(),
                    "fromSurface": true
                });
                if let Some(quality) = options.quality {
                    params["quality"] = Value::from(quality);
                }
                if let Some(clip) = clip {
                    params["clip"] = serde_json::json!({
                        "x": clip.x,
                        "y": clip.y,
                        "width": clip.width,
                        "height": clip.height,
                        "scale": options.scale
                    });
                }
                if options.full_page {
                    let latest = self.cdp.get_layout_metrics().await?;
                    let latest_clip = visual_rect(&latest["cssContentSize"])?;
                    if !visual_clips_match(clip.expect("full-page capture has a clip"), latest_clip) {
                        return Err("full-page geometry changed during capture preparation".into());
                    }
                } else if let Some(target) = options.target.as_deref() {
                    let element = self.resolve_element(target).await?;
                    let model = match (element.node_id, element.backend_dom_node_id) {
                        (Some(node_id), _) => self.cdp.get_box_model(node_id).await?,
                        (_, Some(backend_id)) => self.cdp.get_box_model_for_backend(backend_id).await?,
                        _ => return Err("visual target has no DOM node identity".into()),
                    };
                    let latest_metrics = self.cdp.get_layout_metrics().await?;
                    let viewport = visual_viewport_rect(&latest_metrics["cssVisualViewport"])?;
                    let mut latest_clip = visual_quad_rect(&model["model"]["border"])?;
                    latest_clip.x += viewport.x;
                    latest_clip.y += viewport.y;
                    if !visual_clips_match(clip.expect("element capture has a clip"), latest_clip) {
                        return Err("element geometry changed during capture preparation".into());
                    }
                }
                let data = self.cdp.screenshot_with_params(params).await?;
                if data.len() > MAX_VISUAL_BASE64_BYTES {
                    return Err("visual base64 payload exceeded 64 MiB".into());
                }
                let encoded_bytes = decoded_base64_len(&data)?;
                let header_end = data.len().min(VISUAL_HEADER_BASE64_BYTES) / 4 * 4;
                let header = STANDARD.decode(&data.as_bytes()[..header_end])?;
                let size = imagesize::blob_size(&header)?;
                let (target_id, frame_id) = self.route_identity().await?;
                Ok(VisualCapture {
                    metadata: VisualCaptureMetadata {
                        format: options.format,
                        width: size.width,
                        height: size.height,
                        encoded_bytes,
                        device_scale_factor: dpr,
                        scale: options.scale,
                        full_page: options.full_page,
                        clip,
                        target_id,
                        frame_id,
                    },
                    data,
                })
            })
            .await
    }

    pub async fn start_screencast(
        &self,
        format: VisualFormat,
        quality: u8,
        max_width: u32,
        max_height: u32,
    ) -> BrowserResult<ScreencastScope> {
        self.policy.require(PolicyCapability::Screenshot)?;
        if format == VisualFormat::Webp {
            return Err("CDP screencast supports only png or jpeg".into());
        }
        if quality > 100
            || max_width == 0
            || max_height == 0
            || max_width > 4096
            || max_height > 4096
            || f64::from(max_width) * f64::from(max_height) > MAX_VISUAL_PIXELS
        {
            return Err(
                "screencast quality must be 0..=100 and dimensions must fit the 8 MP budget".into(),
            );
        }
        let session_id = self.cdp.current_session_id();
        let receiver = self.cdp.open_screencast_channel(session_id.clone())?;
        let mut startup = ScreencastStartupGuard {
            cdp: self.cdp.clone(),
            session_id: session_id.clone(),
            armed: true,
        };
        let parameters = Some(serde_json::json!({
            "format": format.as_cdp(),
            "quality": quality,
            "maxWidth": max_width,
            "maxHeight": max_height,
            "everyNthFrame": 1
        }));
        let start_result = match session_id.as_deref() {
            Some(session_id) => {
                self.cdp
                    .send_to_session(session_id, "Page.startScreencast", parameters)
                    .await
            }
            None => self.cdp.send("Page.startScreencast", parameters).await,
        };
        if let Err(error) = start_result {
            return Err(error.into());
        }
        startup.disarm();
        Ok(ScreencastScope {
            cdp: self.cdp.clone(),
            session_id,
            receiver,
            armed: true,
        })
    }
}

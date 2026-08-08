//! Action primitives: clicks, typing, keyboard, scroll, drag.
//!
//! Implementation of individual browser interaction actions on
//! [`BrowserSession`]: click, double-click, hover, drag, key press,
//! scroll, clear, check, uncheck, select, and file upload.

use super::*;

pub(crate) fn media_control_intent(target: &str) -> BrowserResult<Option<bool>> {
    let Locator::RoleAndName { role, name } = Locator::parse(target)? else {
        return Ok(None);
    };
    if !role.eq_ignore_ascii_case("button") {
        return Ok(None);
    }
    let normalized = name.to_ascii_lowercase();
    Ok(
        if normalized == "play" || normalized.starts_with("play (") {
            Some(true)
        } else if normalized == "pause" || normalized.starts_with("pause (") {
            Some(false)
        } else {
            None
        },
    )
}

impl BrowserSession {
    /// Click exact frame-local viewport coordinates. This is an explicit,
    /// policy-gated escape hatch for canvas and map surfaces where no DOM
    /// control can be published. Coordinates are validated against the live
    /// viewport and are never adjusted to a nearby element.
    pub async fn click_at(&self, x: f64, y: f64) -> BrowserResult<CoordinateClickOutcome> {
        self.click_at_with_revision(x, y, None).await
    }

    /// Click viewport coordinates only if the semantic observation revision is
    /// still current. Remote View uses this to reject stale visual input.
    pub async fn click_at_with_revision(
        &self,
        x: f64,
        y: f64,
        expected_revision: Option<u64>,
    ) -> BrowserResult<CoordinateClickOutcome> {
        self.policy
            .require(crate::browser::policy::PolicyCapability::CoordinateClick)?;
        if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
            return Err("click-at coordinates must be finite and non-negative".into());
        }

        self.cdp
            .with_current_route(async {
                self.require_expected_revision(expected_revision)?;
                let hit = self
                    .evaluate_value(&format!(
                        "(() => {{ if ({x} >= innerWidth || {y} >= innerHeight) return null; const e = document.elementFromPoint({x}, {y}); if (!e) return null; return {{tag:e.tagName.toLowerCase(), role:e.getAttribute('role'), name:e.getAttribute('aria-label') || e.textContent?.trim().slice(0, 160) || null}}; }})()"
                    ))
                    .await?;
                let hit = if hit.is_null() {
                    None
                } else {
                    Some(CoordinateHit {
                        tag: hit["tag"].as_str().unwrap_or("unknown").to_string(),
                        role: hit["role"].as_str().map(str::to_string),
                        name: hit["name"].as_str().map(str::to_string),
                    })
                };
                if hit.is_none() {
                    return Err("click-at coordinates are outside the viewport or hit no element".into());
                }
                self.cdp
                    .dispatch_mouse_event("mouseMoved", x, y, None, None)
                    .await?;
                self.cdp
                    .dispatch_mouse_event("mousePressed", x, y, Some("left"), Some(1))
                    .await?;
                self.cdp
                    .dispatch_mouse_event("mouseReleased", x, y, Some("left"), Some(1))
                    .await?;
                let (target_id, frame_id) = self.ensured_route_identity().await?;
                self.mark_lifecycle_phase(LifecyclePhase::ActionVerified);
                Ok(CoordinateClickOutcome {
                    x,
                    y,
                    hit,
                    execution_id: self.next_execution_id(),
                    revision: self.invalidate_observation().await,
                    target_id,
                    frame_id,
                })
            })
            .await
    }

    /// Scroll the viewport by the given pixel offsets.
    ///
    /// Positive `dy` scrolls down; positive `dx` scrolls right.
    pub async fn scroll(&self, dx: f64, dy: f64) -> BrowserResult<ActionOutcome> {
        self.scroll_with_revision(dx, dy, None).await
    }

    /// Scroll while enforcing an optional observation revision.
    pub async fn scroll_with_revision(
        &self,
        dx: f64,
        dy: f64,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(expected_revision)?;
                let previous_revision = self.page_revision.load(Ordering::Relaxed);
                self.cdp.scroll_by(dx, dy).await?;
                let (target_id, frame_id) = self.ensured_route_identity().await?;
                let current_revision = self.invalidate_observation().await;
                self.mark_lifecycle_phase(LifecyclePhase::ActionVerified);
                Ok(ActionOutcome {
                    status: ActionStatus::Succeeded,
                    action: ActionKind::Scroll,
                    execution_id: self.next_execution_id(),
                    target: None,
                    revision: current_revision,
                    previous_revision,
                    current_revision,
                    target_id,
                    frame_id,
                    verification: ActionVerificationEvidence {
                        revision_delta: current_revision.saturating_sub(previous_revision),
                        ..ActionVerificationEvidence::default()
                    },
                    evidence: None,
                })
            })
            .await
    }

    /// Capture the full accessibility tree snapshot for the current page.
    ///
    /// Returns the page info, accessibility roots, and all interactive elements.
    /// Prefer [`observe`](BrowserSession::observe) for compact observations in
    /// agent workflows.
    pub async fn snapshot(&self) -> BrowserResult<AccessibilitySnapshot> {
        self.cdp
            .with_current_route(async {
                let revision = self.page_revision.load(Ordering::Relaxed);
                let raw = serde_json::to_value(self.cdp.get_accessibility_tree().await?)?;
                let roots = parse_accessibility_tree(&raw);
                let context_id = self.page_context_id().await?;
                let interactive =
                    interactive_elements_with_context(&roots, revision, Some(&context_id));
                Ok(AccessibilitySnapshot {
                    page: self.page_info().await?,
                    roots,
                    interactive,
                })
            })
            .await
    }

    /// Click an element and return its structured action outcome.
    pub async fn click(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.pointer_click(ActionRequest::new(ActionKind::Click, target, None))
            .await
    }

    /// Click an element only when the caller's observation revision is current.
    pub async fn click_with_revision(
        &self,
        target: &str,
        expected_revision: u64,
    ) -> BrowserResult<ActionOutcome> {
        self.pointer_click(ActionRequest::new(
            ActionKind::Click,
            target,
            Some(expected_revision),
        ))
        .await
    }

    /// Double-click an element with the same target, scroll, and pointer
    /// contract as a single click.
    pub async fn double_click(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.pointer_click(ActionRequest::new(ActionKind::DoubleClick, target, None))
            .await
    }

    /// Double-click while enforcing an optional observation revision.
    pub async fn double_click_with_revision(
        &self,
        target: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.pointer_click(ActionRequest::new(
            ActionKind::DoubleClick,
            target,
            expected_revision,
        ))
        .await
    }

    /// Hover the pointer over an element without clicking.
    ///
    /// Resolves the target, moves the pointer to the element's center using the
    /// configured interaction mode, then returns an [`ActionOutcome`].
    pub async fn hover(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await?;
                let remote = RemoteObjectGuard::new(self.cdp.clone(), object_id);
                let local = self.verified_action_point(&remote.object_id).await?;
                let point = self.target_viewport_point(local).await?;
                self.move_pointer(point).await?;
                self.action_outcome(ActionKind::Hover, Some(element), None)
                    .await
            })
            .await
    }

    /// Drag an element from `source` to `destination`.
    ///
    /// Performs a mouse-press on the source element, moves the pointer to the
    /// destination element, then releases.
    pub async fn drag(&self, source: &str, destination: &str) -> BrowserResult<ActionOutcome> {
        self.drag_with_revision(source, destination, None).await
    }

    /// Drag while enforcing an optional observation revision.
    pub async fn drag_with_revision(
        &self,
        source: &str,
        destination: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(expected_revision)?;
                let source = self.resolve_element(source).await?;
                let source_object = self
                    .cdp
                    .resolve_node_object(source.node_id, source.backend_dom_node_id)
                    .await?;
                let source_guard = RemoteObjectGuard::new(self.cdp.clone(), source_object);
                let destination = self.resolve_element(destination).await?;
                let destination_object = self
                    .cdp
                    .resolve_node_object(destination.node_id, destination.backend_dom_node_id)
                    .await?;
                let destination_guard =
                    RemoteObjectGuard::new(self.cdp.clone(), destination_object);
                let source_local = self.verified_action_point(&source_guard.object_id).await?;
                let destination_local = self
                    .verified_action_point(&destination_guard.object_id)
                    .await?;
                let source_point = self.target_viewport_point(source_local).await?;
                let destination_point = self.target_viewport_point(destination_local).await?;
                self.move_pointer(source_point).await?;
                let verified_source = self.verified_action_point(&source_guard.object_id).await?;
                if (verified_source.x - source_local.x).abs() > 1.0
                    || (verified_source.y - source_local.y).abs() > 1.0
                {
                    return Err(TargetError {
                        kind: TargetErrorKind::NotActionable,
                        reason: Some(TargetActionabilityReason::GeometryChanged),
                        candidates: Vec::new(),
                        recovery: None,
                        diagnostics: None,
                    }
                    .into());
                }
                self.cdp
                    .dispatch_mouse_event(
                        "mousePressed",
                        source_point.x,
                        source_point.y,
                        Some("left"),
                        Some(1),
                    )
                    .await?;
                let mut pressed = PressedButtonGuard {
                    cdp: self.cdp.clone(),
                    point: source_point,
                    click_count: 1,
                    armed: true,
                };
                let drag_path = interaction_path(
                    self.interaction_mode,
                    &self.mouse,
                    source_point,
                    destination_point,
                );
                for window in drag_path.windows(2) {
                    let point = window[1];
                    if self.interaction_mode == InteractionMode::Human {
                        tokio::time::sleep(self.mouse.move_delay(window[0], point)).await;
                    }
                    self.cdp
                        .dispatch_mouse_event("mouseMoved", point.x, point.y, Some("left"), Some(1))
                        .await?;
                }
                let verified_destination = self
                    .verified_action_point(&destination_guard.object_id)
                    .await?;
                if (verified_destination.x - destination_local.x).abs() > 1.0
                    || (verified_destination.y - destination_local.y).abs() > 1.0
                {
                    return Err(TargetError {
                        kind: TargetErrorKind::NotActionable,
                        reason: Some(TargetActionabilityReason::GeometryChanged),
                        candidates: Vec::new(),
                        recovery: None,
                        diagnostics: None,
                    }
                    .into());
                }
                self.cdp
                    .dispatch_mouse_event(
                        "mouseReleased",
                        destination_point.x,
                        destination_point.y,
                        Some("left"),
                        Some(1),
                    )
                    .await?;
                pressed.armed = false;
                *self.pointer.lock().await = Some(destination_point);
                self.action_outcome(ActionKind::Drag, Some(source), None)
                    .await
            })
            .await
    }

    /// Press and hold a keyboard key.
    ///
    /// Dispatches a `rawKeyDown` CDP event for the given key.
    pub async fn key_down(&self, key: &str) -> BrowserResult<ActionOutcome> {
        self.key_down_with_revision(key, None).await
    }

    /// Press and hold a keyboard key while enforcing an optional revision.
    pub async fn key_down_with_revision(
        &self,
        key: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.keyboard_action(ActionKind::KeyDown, key, "rawKeyDown", 0, expected_revision)
            .await
    }

    /// Release a keyboard key.
    ///
    /// Dispatches a `keyUp` CDP event for the given key.
    pub async fn key_up(&self, key: &str) -> BrowserResult<ActionOutcome> {
        self.key_up_with_revision(key, None).await
    }

    /// Release a keyboard key while enforcing an optional revision.
    pub async fn key_up_with_revision(
        &self,
        key: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.keyboard_action(ActionKind::KeyUp, key, "keyUp", 0, expected_revision)
            .await
    }

    /// Press and release a keyboard key.
    ///
    /// Dispatches `rawKeyDown`, `char` (for single-character keys), and `keyUp`
    /// CDP events.
    pub async fn key_press(&self, key: &str) -> BrowserResult<ActionOutcome> {
        self.key_press_with_revision(key, None).await
    }

    /// Press and release a keyboard key while enforcing an optional revision.
    pub async fn key_press_with_revision(
        &self,
        key: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        validate_key(key)?;
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(expected_revision)?;
                let code = key_code(key);
                self.cdp
                    .dispatch_key_event_with_modifiers("rawKeyDown", key, &code, "", 0)
                    .await?;
                if key.chars().count() == 1 {
                    self.cdp
                        .dispatch_key_event_with_modifiers("char", key, &code, key, 0)
                        .await?;
                }
                self.cdp
                    .dispatch_key_event_with_modifiers("keyUp", key, &code, "", 0)
                    .await?;
                self.action_outcome(ActionKind::KeyPress, None, None).await
            })
            .await
    }

    /// Execute a keyboard shortcut with modifier keys.
    ///
    /// Parses shortcuts like `"Ctrl+C"` or `"Meta+V"` and dispatches
    /// the corresponding key events with the specified modifiers.
    pub async fn shortcut(&self, shortcut: &str) -> BrowserResult<ActionOutcome> {
        self.shortcut_with_revision(shortcut, None).await
    }

    /// Execute a keyboard shortcut while enforcing an optional revision.
    pub async fn shortcut_with_revision(
        &self,
        shortcut: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        let (modifiers, key) = parse_shortcut(shortcut)?;
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(expected_revision)?;
                let code = key_code(&key);
                self.cdp
                    .dispatch_key_event_with_modifiers("rawKeyDown", &key, &code, "", modifiers)
                    .await?;
                self.cdp
                    .dispatch_key_event_with_modifiers("keyUp", &key, &code, "", modifiers)
                    .await?;
                self.action_outcome(ActionKind::Shortcut, None, None).await
            })
            .await
    }

    /// Clear the contents of an editable element.
    ///
    /// Clicks the target, selects all content, then presses Backspace.
    /// Verifies the element is empty afterward.
    pub async fn clear(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.clear_with_revision(target, None).await
    }

    /// Clear an editable element while enforcing an optional revision.
    pub async fn clear_with_revision(
        &self,
        target: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(expected_revision)?;
                let element = self.resolve_element(target).await?;
                let object_id = self.cdp.resolve_node_object(element.node_id, element.backend_dom_node_id).await?;
                let remote = RemoteObjectGuard::new(self.cdp.clone(), object_id);
                let editable = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return this instanceof HTMLInputElement || this instanceof HTMLTextAreaElement || this.isContentEditable}").await?)?;
                if editable.as_bool() != Some(true) { return Err("clear target is not editable".into()); }
                let clicked = self
                    .pointer_click(ActionRequest::new(
                        ActionKind::Click,
                        target,
                        expected_revision,
                    ))
                    .await?;
                self.cdp.dispatch_select_all().await?;
                self.key_press("Backspace").await?;
                let empty = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return this instanceof HTMLInputElement || this instanceof HTMLTextAreaElement ? this.value === '' : this.textContent === ''}").await?)?;
                if empty.as_bool() != Some(true) { return Err("clear target did not become empty".into()); }
                self.action_outcome_from_target(ActionKind::Clear, clicked.target)
                    .await
            })
            .await
    }

    /// Check a checkbox or radio button.
    ///
    /// Ensures the target element's `checked` property is set to `true`.
    pub async fn check(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.set_checked(target, true).await
    }

    /// Check a control while enforcing an optional observation revision.
    pub async fn check_with_revision(
        &self,
        target: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.set_checked_with_revision(target, true, expected_revision)
            .await
    }

    /// Uncheck a checkbox.
    ///
    /// Ensures the target element's `checked` property is set to `false`.
    pub async fn uncheck(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.set_checked(target, false).await
    }

    /// Uncheck a control while enforcing an optional observation revision.
    pub async fn uncheck_with_revision(
        &self,
        target: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.set_checked_with_revision(target, false, expected_revision)
            .await
    }

    /// Select an option from a `<select>` element by value.
    ///
    /// `value` must be 1–4096 bytes. Fires `input` and `change` events.
    pub async fn select_option(&self, target: &str, value: &str) -> BrowserResult<ActionOutcome> {
        self.select_option_with_revision(target, value, None).await
    }

    /// Select an option while enforcing an optional observation revision.
    pub async fn select_option_with_revision(
        &self,
        target: &str,
        value: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        if value.is_empty() || value.len() > 4096 {
            return Err("select value must be 1..=4096 bytes".into());
        }
        let value_json = serde_json::to_string(value)?;
        self.form_object_action(
            target,
            ActionKind::Select,
            &format!(r#"function() {{ if (!(this instanceof HTMLSelectElement)) return {{ok:false,reason:'not_select'}}; const option = Array.from(this.options).find(option => option.value === {value_json}); if (!option) return {{ok:false,reason:'option_not_found'}}; this.value = option.value; this.dispatchEvent(new Event('input',{{bubbles:true}})); this.dispatchEvent(new Event('change',{{bubbles:true}})); return {{ok:this.value === option.value}}; }}"#),
            expected_revision,
        )
        .await
    }

    pub async fn upload_files(
        &self,
        target: &str,
        paths: &[PathBuf],
    ) -> BrowserResult<ActionOutcome> {
        self.upload_files_with_revision(target, paths, None).await
    }

    /// Upload files while enforcing an optional observation revision.
    pub async fn upload_files_with_revision(
        &self,
        target: &str,
        paths: &[PathBuf],
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.policy.require(PolicyCapability::Upload)?;
        self.cdp.with_current_route(async {
            self.require_expected_revision(expected_revision)?;
            if paths.is_empty() || paths.len() > 16 { return Err("upload requires 1..=16 files".into()); }
            let mut files = Vec::with_capacity(paths.len());
            for path in paths {
                let canonical = self.policy.require_existing_path(path)?;
                if !canonical.is_file() { return Err("upload path must be a regular file".into()); }
                if !canonical.starts_with(&self.upload_root) { return Err("upload path is outside the allowed workspace root".into()); }
                files.push(canonical.to_string_lossy().into_owned());
            }
            let element = self.resolve_element(target).await?;
            let object_id = self.cdp.resolve_node_object(element.node_id, element.backend_dom_node_id).await?;
            let remote = RemoteObjectGuard::new(self.cdp.clone(), object_id);
            self.verified_action_point(&remote.object_id).await?;
            let input = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return {ok:this instanceof HTMLInputElement && this.type === 'file'}}").await?)?;
            if input["ok"].as_bool() != Some(true) { return Err("upload target is not a file input".into()); }
            if element.node_id.is_none() && element.backend_dom_node_id.is_none() { return Err("file input target has no DOM node ID".into()); }
            self.cdp.set_file_input_files(element.node_id, element.backend_dom_node_id, &files).await?;
            let verified = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return this.files.length}").await?)?;
            if verified.as_u64() != Some(files.len() as u64) { return Err("file input did not retain the requested file count".into()); }
            let outcome = self.action_outcome(ActionKind::Upload, Some(element), Some(serde_json::json!({"file_count": files.len()}))).await?;
            self.record_audit("upload", format!("{} files", files.len()));
            Ok(outcome)
        }).await
    }

    async fn apply_media_control(&self, target: &str) -> BrowserResult<Option<bool>> {
        let Some(wants_playing) = media_control_intent(target)? else {
            return Ok(None);
        };
        let state = self
            .evaluate_value(
                "(() => { const media = document.querySelector('video, audio'); \
                 if (!media) return null; \
                 const rect = media.getBoundingClientRect(); \
                 return {paused: media.paused, x: rect.left + rect.width / 2, \
                         y: rect.top + rect.height / 2, width: rect.width, height: rect.height}; })()",
            )
            .await?;
        let Some(paused) = state["paused"].as_bool() else {
            return Ok(None);
        };
        if paused != wants_playing {
            return Ok(Some(false));
        }
        let x = state["x"]
            .as_f64()
            .ok_or("media target x was not numeric")?;
        let y = state["y"]
            .as_f64()
            .ok_or("media target y was not numeric")?;
        let width = state["width"].as_f64().unwrap_or(0.0);
        let height = state["height"].as_f64().unwrap_or(0.0);
        if !x.is_finite() || !y.is_finite() || width <= 0.0 || height <= 0.0 {
            return Ok(None);
        }
        let point = self.target_viewport_point(Point { x, y }).await?;
        self.cdp
            .dispatch_mouse_event("mouseMoved", point.x, point.y, None, None)
            .await?;
        self.cdp
            .dispatch_mouse_event("mousePressed", point.x, point.y, Some("left"), Some(1))
            .await?;
        self.cdp
            .dispatch_mouse_event("mouseReleased", point.x, point.y, Some("left"), Some(1))
            .await?;
        Ok(Some(true))
    }

    async fn resolve_click_target(
        &self,
        target: &str,
    ) -> BrowserResult<(ResolvedElement, String, Point)> {
        const MAX_NODE_RESOLUTION_ATTEMPTS: usize = 10;
        let mut last_error: Option<Box<dyn Error>> = None;

        for attempt in 0..MAX_NODE_RESOLUTION_ATTEMPTS {
            let element = self.resolve_element(target).await?;
            let object_id = match self
                .cdp
                .resolve_node_object(element.node_id, element.backend_dom_node_id)
                .await
            {
                Ok(object_id) => object_id,
                Err(error) => {
                    tracing::debug!(%error, attempt, "target node could not be resolved");
                    last_error = Some(Box::new(TargetError {
                        kind: TargetErrorKind::NotActionable,
                        reason: Some(TargetActionabilityReason::NodeUnavailable),
                        candidates: Vec::new(),
                        recovery: None,
                        diagnostics: None,
                    }));
                    if attempt + 1 < MAX_NODE_RESOLUTION_ATTEMPTS {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                    break;
                }
            };

            let local_point = match self.verified_action_point(&object_id).await {
                Ok(point) => point,
                Err(error)
                    if error
                        .downcast_ref::<TargetError>()
                        .and_then(|target_error| target_error.reason)
                        .is_some_and(|reason| {
                            matches!(
                                reason,
                                TargetActionabilityReason::NodeUnavailable
                                    | TargetActionabilityReason::Detached
                                    | TargetActionabilityReason::NotVisible
                                    | TargetActionabilityReason::OutsideViewport
                                    | TargetActionabilityReason::HitTestBlocked
                                    | TargetActionabilityReason::VerificationFailed
                                    | TargetActionabilityReason::GeometryChanged
                                    | TargetActionabilityReason::UnstableGeometry
                            )
                        }) =>
                {
                    if let Some(target_error) = error.downcast_ref::<TargetError>() {
                        tracing::debug!(
                            attempt,
                            ?target_error.reason,
                            ?target_error.diagnostics,
                            "retrying target actionability verification"
                        );
                    }
                    last_error = Some(error);
                    if attempt + 1 < MAX_NODE_RESOLUTION_ATTEMPTS {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                    break;
                }
                Err(error) => {
                    if let Some(target_error) = error.downcast_ref::<TargetError>() {
                        tracing::debug!(
                            ?target_error.reason,
                            ?target_error.diagnostics,
                            "target actionability verification failed"
                        );
                    }
                    return Err(error);
                }
            };

            return Ok((element, object_id, local_point));
        }

        Err(last_error.expect("node resolution retry loop must retain its last error"))
    }

    async fn pointer_click(&self, request: ActionRequest<'_>) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(request.expected_revision)?;
                let previous_revision = self.page_revision.load(Ordering::Relaxed);
                let before = self.page_info().await.ok();
                let media_target = if request.action == ActionKind::Click
                    && media_control_intent(request.target)?.is_some()
                {
                    Some(self.resolve_element(request.target).await?)
                } else {
                    None
                };
                if media_target.is_some()
                    && let Some(media_changed) = self.apply_media_control(request.target).await?
                {
                    let (target_id, frame_id) = self.route_identity().await?;
                    let current_revision = if media_changed {
                        self.invalidate_observation().await
                    } else {
                        previous_revision
                    };
                    self.mark_lifecycle_phase(LifecyclePhase::ActionVerified);
                    return Ok(ActionOutcome {
                        status: ActionStatus::Succeeded,
                        action: request.action,
                        execution_id: self.next_execution_id(),
                        target: Some(ActionTarget {
                            label: request.target.to_string(),
                            reference: None,
                        }),
                        revision: current_revision,
                        previous_revision,
                        current_revision,
                        target_id,
                        frame_id,
                        verification: ActionVerificationEvidence {
                            revision_delta: current_revision.saturating_sub(previous_revision),
                            ..ActionVerificationEvidence::default()
                        },
                        evidence: None,
                    });
                }
                let (element, object_id, local_point) =
                    self.resolve_click_target(request.target).await?;
                let remote = RemoteObjectGuard::new(self.cdp.clone(), object_id);
                let point = self.target_viewport_point(local_point).await?;
                let events = if request.action == ActionKind::DoubleClick {
                    self.mouse.generate_double_click_events(point)
                } else {
                    self.mouse.generate_click_events(point)
                };
                self.dispatch_pointer_events(&remote.object_id, local_point, point, events)
                    .await?;
                let (target_id, frame_id) = self.route_identity().await?;
                let current_revision = self.invalidate_observation().await;
                let after = self.page_info().await.ok();
                self.mark_lifecycle_phase(LifecyclePhase::ActionVerified);
                Ok(ActionOutcome {
                    status: ActionStatus::Succeeded,
                    action: request.action,
                    execution_id: self.next_execution_id(),
                    target: Some(ActionTarget {
                        label: element.label,
                        reference: element.reference,
                    }),
                    revision: current_revision,
                    previous_revision,
                    current_revision,
                    target_id,
                    frame_id,
                    verification: ActionVerificationEvidence {
                        revision_delta: current_revision.saturating_sub(previous_revision),
                        url_changed: before
                            .as_ref()
                            .zip(after.as_ref())
                            .is_some_and(|(before, after)| before.url != after.url),
                        title_changed: before
                            .as_ref()
                            .zip(after.as_ref())
                            .is_some_and(|(before, after)| before.title != after.title),
                        target_changed: before
                            .as_ref()
                            .zip(after.as_ref())
                            .is_some_and(|(before, after)| before.target_id != after.target_id),
                        frame_changed: before
                            .as_ref()
                            .zip(after.as_ref())
                            .is_some_and(|(before, after)| before.frame_id != after.frame_id),
                        ..ActionVerificationEvidence::default()
                    },
                    evidence: None,
                })
            })
            .await
    }

    async fn dispatch_pointer_events(
        &self,
        object_id: &str,
        local_point: Point,
        point: Point,
        events: Vec<crate::browser::mouse::MouseEvent>,
    ) -> BrowserResult<()> {
        let mut pointer = self.pointer.lock().await;
        let start = match (self.interaction_mode, *pointer) {
            (_, Some(point)) => point,
            (InteractionMode::Human, None) => self
                .viewport_center()
                .await
                .unwrap_or(Point { x: 640.0, y: 360.0 }),
            (InteractionMode::Fast, None) => point,
        };
        let path = interaction_path(self.interaction_mode, &self.mouse, start, point);
        if self.interaction_mode == InteractionMode::Human && pointer.is_none() {
            self.cdp
                .dispatch_mouse_event("mouseMoved", start.x, start.y, None, None)
                .await?;
        }
        for window in path.windows(2) {
            let next = window[1];
            if self.interaction_mode == InteractionMode::Human {
                tokio::time::sleep(self.mouse.move_delay(window[0], next)).await;
            }
            self.cdp
                .dispatch_mouse_event("mouseMoved", next.x, next.y, None, None)
                .await?;
        }
        let press_point = self.verified_action_point(object_id).await?;
        if (press_point.x - local_point.x).abs() > 1.0
            || (press_point.y - local_point.y).abs() > 1.0
        {
            return Err(TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(TargetActionabilityReason::GeometryChanged),
                candidates: Vec::new(),
                recovery: None,
                diagnostics: None,
            }
            .into());
        }
        let mut pressed = None;
        for event in events {
            if event.event_type == "mousePressed" {
                pressed = Some(PressedButtonGuard {
                    cdp: self.cdp.clone(),
                    point,
                    click_count: event.click_count,
                    armed: true,
                });
            }
            self.cdp
                .dispatch_mouse_event(
                    &event.event_type,
                    event.x,
                    event.y,
                    Some(&event.button),
                    Some(event.click_count),
                )
                .await?;
            if event.event_type == "mouseReleased"
                && let Some(mut guard) = pressed.take()
            {
                guard.armed = false;
            }
            if self.interaction_mode == InteractionMode::Human && event.event_type == "mousePressed"
            {
                tokio::time::sleep(self.mouse.click_delay()).await;
            }
        }
        *pointer = Some(point);
        Ok(())
    }

    /// Type text into the page.
    ///
    /// If `target` is provided, clicks the target element first to focus it,
    /// then inserts the text via CDP `Input.insertText`. Otherwise types at
    /// the current focus.
    pub async fn type_text(
        &self,
        text: &str,
        target: Option<&str>,
    ) -> BrowserResult<ActionOutcome> {
        self.type_text_with_expected_revision(text, target, None)
            .await
    }

    /// Type text while enforcing an optional observation revision.
    pub async fn type_text_with_expected_revision(
        &self,
        text: &str,
        target: Option<&str>,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(expected_revision)?;
                let previous_revision = self.page_revision.load(Ordering::Relaxed);
                let target = match target {
                    Some(target) => {
                        self.pointer_click(ActionRequest::new(
                            ActionKind::Click,
                            target,
                            expected_revision,
                        ))
                        .await?
                        .target
                    }
                    None => None,
                };
                self.cdp.insert_text(text).await?;
                let (target_id, frame_id) = self.route_identity().await?;
                let current_revision = self.invalidate_observation().await;
                self.mark_lifecycle_phase(LifecyclePhase::ActionVerified);
                Ok(ActionOutcome {
                    status: ActionStatus::Succeeded,
                    action: ActionKind::Type,
                    execution_id: self.next_execution_id(),
                    target,
                    revision: current_revision,
                    previous_revision,
                    current_revision,
                    target_id,
                    frame_id,
                    verification: ActionVerificationEvidence {
                        revision_delta: current_revision.saturating_sub(previous_revision),
                        ..ActionVerificationEvidence::default()
                    },
                    evidence: None,
                })
            })
            .await
    }

    async fn move_pointer(&self, destination: Point) -> BrowserResult<()> {
        let mut pointer = self.pointer.lock().await;
        let start = pointer.unwrap_or(destination);
        for window in
            interaction_path(self.interaction_mode, &self.mouse, start, destination).windows(2)
        {
            if self.interaction_mode == InteractionMode::Human {
                tokio::time::sleep(self.mouse.move_delay(window[0], window[1])).await;
            }
            self.cdp
                .dispatch_mouse_event("mouseMoved", window[1].x, window[1].y, None, None)
                .await?;
        }
        if start == destination {
            self.cdp
                .dispatch_mouse_event("mouseMoved", destination.x, destination.y, None, None)
                .await?;
        }
        *pointer = Some(destination);
        Ok(())
    }

    async fn keyboard_action(
        &self,
        action: ActionKind,
        key: &str,
        event_type: &str,
        modifiers: i64,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        validate_key(key)?;
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(expected_revision)?;
                self.cdp
                    .dispatch_key_event_with_modifiers(
                        event_type,
                        key,
                        &key_code(key),
                        "",
                        modifiers,
                    )
                    .await?;
                self.action_outcome(action, None, None).await
            })
            .await
    }

    async fn set_checked(&self, target: &str, checked: bool) -> BrowserResult<ActionOutcome> {
        self.set_checked_with_revision(target, checked, None).await
    }

    async fn set_checked_with_revision(
        &self,
        target: &str,
        checked: bool,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        let action = if checked {
            ActionKind::Check
        } else {
            ActionKind::Uncheck
        };
        let script = format!(
            r#"function() {{ if (!(this instanceof HTMLInputElement) || !['checkbox','radio'].includes(this.type)) return {{ok:false,reason:'not_checkable'}}; if (this.checked !== {checked}) this.click(); return {{ok:this.checked === {checked}}}; }}"#
        );
        self.form_object_action(target, action, &script, expected_revision)
            .await
    }

    async fn form_object_action(
        &self,
        target: &str,
        action: ActionKind,
        function: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(expected_revision)?;
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await?;
                let remote = RemoteObjectGuard::new(self.cdp.clone(), object_id);
                self.verified_action_point(&remote.object_id).await?;
                let result = self.cdp.call_on_object(&remote.object_id, function).await?;
                let value = runtime_value(&result)?;
                if value["ok"].as_bool() != Some(true) {
                    let current_revision = self.page_revision.load(Ordering::Relaxed);
                    return Err(ActionVerificationError {
                        kind: ActionFailureKind::VerificationFailed,
                        action,
                        phase: ActionFailurePhase::Verification,
                        recovery_strategy: RecoveryStrategy::Report,
                        execution_id: Some(self.next_execution_id()),
                        target: Some(ActionTarget {
                            label: element.label,
                            reference: element.reference,
                        }),
                        revision: current_revision,
                        reason: value["reason"]
                            .as_str()
                            .unwrap_or("verification_failed")
                            .to_string(),
                    }
                    .into());
                }
                self.action_outcome(action, Some(element), None).await
            })
            .await
    }

    async fn action_outcome(
        &self,
        action: ActionKind,
        element: Option<ResolvedElement>,
        evidence: Option<Value>,
    ) -> BrowserResult<ActionOutcome> {
        let target = element.map(|element| ActionTarget {
            label: element.label,
            reference: element.reference,
        });
        let mut outcome = self.action_outcome_from_target(action, target).await?;
        outcome.evidence = evidence;
        Ok(outcome)
    }

    pub(crate) async fn action_outcome_from_target(
        &self,
        action: ActionKind,
        target: Option<ActionTarget>,
    ) -> BrowserResult<ActionOutcome> {
        let previous_revision = self.page_revision.load(Ordering::Relaxed);
        let before_page = self
            .observation_cache
            .lock()
            .await
            .as_ref()
            .map(|cached| cached.context.page.clone());
        let (before_popup_count, before_dialog_open, before_download_sequence) = {
            let topology = self.topology.lock().await;
            (
                topology.targets.len(),
                topology.pending_dialog.is_some(),
                self.download_sequence.load(Ordering::Relaxed),
            )
        };
        if let Some(interception) = &self.policy_interception {
            // A same-route command is an ordering barrier for synchronous
            // click/form navigation. The interception itself remains active
            // for delayed page-authored navigation after this action returns.
            let _ = self.cdp.evaluate("0").await;
            tokio::task::yield_now().await;
            if let Some(error) = interception.take_denial().await {
                return Err(error.into());
            }
        }
        let (target_id, frame_id) = self.route_identity().await?;
        let after_page = self.page_info().await.ok();
        let (popup_opened, dialog_open) = {
            let topology = self.topology.lock().await;
            (
                before_popup_count > 0 && topology.targets.len() > before_popup_count,
                topology.pending_dialog.is_some() && !before_dialog_open,
            )
        };
        let download_started =
            self.download_sequence.load(Ordering::Relaxed) > before_download_sequence;
        let current_revision = self.invalidate_observation().await;
        let outcome = ActionOutcome {
            status: ActionStatus::Succeeded,
            action,
            execution_id: self.next_execution_id(),
            target,
            revision: current_revision,
            previous_revision,
            current_revision,
            target_id,
            frame_id,
            verification: ActionVerificationEvidence {
                revision_delta: current_revision.saturating_sub(previous_revision),
                url_changed: before_page
                    .as_ref()
                    .zip(after_page.as_ref())
                    .is_some_and(|(before, after)| before.url != after.url),
                title_changed: before_page
                    .as_ref()
                    .zip(after_page.as_ref())
                    .is_some_and(|(before, after)| before.title != after.title),
                target_changed: before_page
                    .as_ref()
                    .zip(after_page.as_ref())
                    .is_some_and(|(before, after)| before.target_id != after.target_id),
                frame_changed: before_page
                    .as_ref()
                    .zip(after_page.as_ref())
                    .is_some_and(|(before, after)| before.frame_id != after.frame_id),
                popup_opened,
                dialog_open,
                download_started,
                ..ActionVerificationEvidence::default()
            },
            evidence: None,
        };
        self.mark_lifecycle_phase(LifecyclePhase::ActionVerified);
        Ok(outcome)
    }

    pub(crate) async fn viewport_center(&self) -> BrowserResult<Point> {
        let value = self
            .evaluate_value("[window.innerWidth / 2, window.innerHeight / 2]")
            .await?;
        let coordinates = value
            .as_array()
            .filter(|coordinates| coordinates.len() == 2)
            .ok_or("viewport evaluation returned invalid coordinates")?;
        let x = coordinates[0]
            .as_f64()
            .ok_or("viewport width was not numeric")?;
        let y = coordinates[1]
            .as_f64()
            .ok_or("viewport height was not numeric")?;
        Ok(Point { x, y })
    }

    pub(crate) async fn target_viewport_point(&self, point: Point) -> BrowserResult<Point> {
        let Some(frame_id) = self.cdp.active_frame() else {
            return Ok(point);
        };
        let frame = {
            let topology = self.topology.lock().await;
            topology
                .frames
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned()
        };
        let frame = match frame {
            Some(frame) => frame,
            None => self
                .list_frames()
                .await?
                .into_iter()
                .find(|frame| frame.id == frame_id)
                .ok_or("selected frame is no longer attached")?,
        };
        if frame.parent_id.is_none() {
            return Ok(point);
        }
        let (x, y) = self.cdp.frame_viewport_offset(&frame_id).await?;
        Ok(Point {
            x: point.x + x,
            y: point.y + y,
        })
    }

    pub(crate) async fn evaluate_value(&self, expression: &str) -> BrowserResult<Value> {
        let raw = self.cdp.evaluate(expression).await?;
        runtime_value(&raw)
    }

    pub(crate) async fn page_context_id(&self) -> BrowserResult<String> {
        self.current_page_context_id().await
    }

    pub(crate) async fn invalidate_observation(&self) -> u64 {
        let revision = self.page_revision.fetch_add(1, Ordering::Relaxed) + 1;
        let expression = format!("globalThis.__glassPageRevision = {revision}; {revision}");
        let _ = self.cdp.evaluate(&expression).await;
        revision
    }

    pub(crate) async fn synchronize_persistent_revision(&self) -> BrowserResult<()> {
        let revision = self
            .evaluate_value(
                "(() => { \
                    const key = '__glassPageRevision'; \
                    const current = Number(globalThis[key]); \
                    if (!Number.isSafeInteger(current) || current < 1) globalThis[key] = 1; \
                    return Number(globalThis[key]); \
                })()",
            )
            .await?
            .as_u64()
            .unwrap_or(1)
            .max(1);
        self.page_revision.store(revision, Ordering::Relaxed);
        Ok(())
    }

    pub(crate) fn require_expected_revision(
        &self,
        expected_revision: Option<u64>,
    ) -> BrowserResult<()> {
        if let Some(expected_revision) = expected_revision {
            let current_revision = self.page_revision.load(Ordering::Relaxed);
            if expected_revision != current_revision {
                return Err(ActionContractError::stale_revision_with_execution(
                    expected_revision,
                    current_revision,
                    self.next_execution_id(),
                )
                .into());
            }
        }
        Ok(())
    }

    pub(crate) async fn verified_action_point(&self, object_id: &str) -> BrowserResult<Point> {
        let raw = match self.cdp.call_on_object(object_id, HIT_TEST_FUNCTION).await {
            Ok(raw) => raw,
            Err(error) => {
                tracing::debug!(%error, "target node could not be verified");
                return Err(TargetError {
                    kind: TargetErrorKind::NotActionable,
                    reason: Some(TargetActionabilityReason::NodeUnavailable),
                    candidates: Vec::new(),
                    recovery: None,
                    diagnostics: None,
                }
                .into());
            }
        };
        let value = runtime_value(&raw)?;
        if value["ok"].as_bool() != Some(true) {
            let reason = value["reason"].as_str().unwrap_or("verification_failed");
            tracing::debug!(reason, "target actionability check failed");
            let diagnostics = serde_json::from_value(value["diagnostics"].clone()).ok();
            return Err(TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(actionability_reason(reason)),
                candidates: Vec::new(),
                recovery: None,
                diagnostics,
            }
            .into());
        }
        let x = value["x"]
            .as_f64()
            .ok_or("verified target x was not numeric")?;
        let y = value["y"]
            .as_f64()
            .ok_or("verified target y was not numeric")?;
        Ok(Point { x, y })
    }
}

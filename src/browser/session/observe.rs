//! Compact page observation.
//!
//! Produces a bounded [`CompactAccessibilitySnapshot`] with visible text,
//! interactive element summaries, and optional full DOM / screenshot /
//! form-value overlays.

use super::*;

impl BrowserSession {
    /// Return the visible text content of the current page.
    ///
    /// Evaluates `document.body.innerText` and truncates the result to
    /// [`COMPACT_TEXT_MAX_BYTES`] (16 KiB).
    pub async fn text(&self) -> BrowserResult<String> {
        self.cdp
            .with_current_route(async {
                let value = self
                    .evaluate_value("document.body ? document.body.innerText : ''")
                    .await?;
                Ok(truncate_visible_text(
                    value.as_str().unwrap_or_default(),
                    COMPACT_TEXT_MAX_BYTES,
                ))
            })
            .await
    }

    /// Fetch the full DOM only for an explicit deep-inspection operation.
    pub async fn deep_dom(&self) -> BrowserResult<DomNode> {
        self.cdp
            .with_current_route(async {
                let raw = serde_json::to_value(self.cdp.get_deep_document().await?)?;
                parse_dom_tree(&raw).ok_or_else(|| {
                    "CDP deep DOM response contained no parseable root node"
                        .to_string()
                        .into()
                })
            })
            .await
    }

    /// Collect compact page context without a deep DOM or screenshot.
    pub async fn observe(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, true, false, CompactRanking::Relevance)
            .await
    }

    /// Collect compact context and explicitly include the full DOM tree.
    pub async fn observe_with_dom(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, false, true, false, CompactRanking::Relevance)
            .await
    }

    /// Collect structured context and explicitly include a current screenshot.
    pub async fn observe_with_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, true, true, false, CompactRanking::Relevance)
            .await
    }

    /// Collect context with both explicitly requested deep DOM and screenshot data.
    pub async fn observe_with_dom_and_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, true, true, false, CompactRanking::Relevance)
            .await
    }

    /// Collect fresh compact context, bypassing the compact-context cache.
    pub async fn observe_fresh(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, false, false, CompactRanking::Relevance)
            .await
    }

    /// Collect compact context with form field values included.
    /// Requires ReadFormValues policy capability in hardened mode.
    pub async fn observe_with_form_values(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, false, true, CompactRanking::Relevance)
            .await
    }

    /// Collect fresh context and explicitly include the full DOM tree.
    pub async fn observe_fresh_with_dom(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, false, false, false, CompactRanking::Relevance)
            .await
    }

    /// Collect fresh structured context and explicitly include a screenshot.
    pub async fn observe_fresh_with_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, true, false, false, CompactRanking::Relevance)
            .await
    }

    /// Collect fresh context with both explicitly requested deep DOM and screenshot data.
    pub async fn observe_fresh_with_dom_and_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, true, false, false, CompactRanking::Relevance)
            .await
    }

    /// Collect compact context with an explicit truncation ordering.
    pub async fn observe_with_ranking(
        &self,
        ranking: ObservationRanking,
    ) -> BrowserResult<PageContext> {
        let ranking = match ranking {
            ObservationRanking::Relevance => CompactRanking::Relevance,
            ObservationRanking::DocumentOrder => CompactRanking::DocumentOrder,
        };
        self.observe_internal(false, false, false, false, ranking)
            .await
    }

    async fn observe_internal(
        &self,
        include_dom: bool,
        include_screenshot: bool,
        use_cache: bool,
        include_form_values: bool,
        ranking: CompactRanking,
    ) -> BrowserResult<PageContext> {
        if let Some(interception) = &self.policy_interception
            && let Some(error) = interception.take_denial().await
        {
            return Err(error.into());
        }
        self.ensured_route_identity().await?;
        let mut context = self
            .compact_observation(use_cache, include_form_values, ranking)
            .await?
            .into_page_context();
        if include_dom {
            context.dom = Some(self.deep_dom().await?);
        }
        if include_screenshot {
            context.screenshot = Some(self.screenshot_base64().await?);
        }
        Ok(context)
    }

    async fn compact_observation(
        &self,
        use_cache: bool,
        include_form_values: bool,
        ranking: CompactRanking,
    ) -> BrowserResult<CompactPageContext> {
        let revision = self.page_revision.load(Ordering::Relaxed);
        // Never use cache when form values are requested (cache doesn't store them)
        if use_cache && !include_form_values {
            let cached_context = {
                let cache = self.observation_cache.lock().await;
                cache
                    .as_ref()
                    .filter(|cached| cached.revision == revision)
                    .map(|cached| cached.context.clone())
            };
            if let Some(context) = cached_context {
                return Ok(context);
            }
        }

        let (target_id, frame_id) = self.route_identity().await?;
        let mut context_id = self.observation_context_id(&target_id, &frame_id).await?;
        let mut collected = None;
        let mut attempt = 1;
        let mut recovered_context = false;
        while attempt <= COMPACT_OBSERVATION_MAX_ATTEMPTS {
            let start_revision = self.page_revision.load(Ordering::Relaxed);
            let attempt_result =
                match tokio::time::timeout(COMPACT_OBSERVATION_ATTEMPT_TIMEOUT, async {
                    let start = self.compact_page_state(context_id).await?;
                    // Keep the mutation bracket intact while overlapping the
                    // independent accessibility and page-state reads. The
                    // start/end snapshots still gate consistency; only their
                    // CDP round trips are allowed to share a flight.
                    let (accessibility, end) = tokio::join!(
                        self.cached_accessibility_tree(&target_id, &frame_id, start_revision),
                        self.compact_page_state(context_id),
                    );
                    let accessibility = accessibility?;
                    let end = end?;
                    BrowserResult::Ok((start, accessibility, end))
                })
                .await
                {
                    Err(_) => {
                        return Err(format!(
                            "compact observation attempt exceeded its {}ms deadline",
                            COMPACT_OBSERVATION_ATTEMPT_TIMEOUT.as_millis()
                        )
                        .into());
                    }
                    Ok(Ok(result)) => result,
                    Ok(Err(error))
                        if is_stale_observation_context(error.as_ref()) && !recovered_context =>
                    {
                        self.discard_observation_context(&target_id, &frame_id, context_id)
                            .await;
                        context_id = self.observation_context_id(&target_id, &frame_id).await?;
                        recovered_context = true;
                        continue;
                    }
                    Ok(Err(error)) => return Err(error),
                };
            let end_revision = self.page_revision.load(Ordering::Relaxed);
            let consistent = start_revision == end_revision
                && attempt_result.0.mutation_revision == attempt_result.2.mutation_revision
                && attempt_result.0.page_context_id == attempt_result.2.page_context_id;
            collected = Some((
                attempt,
                consistent,
                start_revision,
                end_revision,
                attempt_result,
            ));
            if consistent {
                break;
            }
            attempt += 1;
        }
        let (
            attempts,
            consistent,
            start_revision,
            end_revision,
            (start_state, accessibility_raw, page_state),
        ) = collected.expect("observation always performs at least one attempt");
        let page = PageInfo {
            url: page_state.url,
            title: page_state.title,
            ready_state: page_state.ready_state,
            target_id: target_id.clone(),
            frame_id: frame_id.clone(),
        };
        let full_roots = parse_accessibility_tree(&accessibility_raw);
        let page_context_id =
            (!page_state.page_context_id.is_empty()).then_some(page_state.page_context_id.as_str());
        let mut compact_accessibility =
            crate::browser::dom::project_compact_accessibility_with_ranking_and_context(
                &full_roots,
                end_revision,
                page_context_id,
                ranking,
            );
        let (mut text, locally_truncated) =
            truncate_visible_text_with_status(&page_state.text, COMPACT_TEXT_MAX_BYTES);
        let text_truncated = locally_truncated || page_state.boundaries.text_truncated;
        if page_state.boundaries.text_truncated && !text.ends_with(TEXT_TRUNCATION_MARKER) {
            let content_limit = COMPACT_TEXT_MAX_BYTES.saturating_sub(TEXT_TRUNCATION_MARKER.len());
            while text.len() > content_limit {
                text.pop();
            }
            text.push_str(TEXT_TRUNCATION_MARKER);
        }
        let mut incomplete = Vec::new();
        if text_truncated {
            incomplete.push(ObservationIncompleteReason::VisibleText);
        }
        if compact_accessibility.nodes_truncated {
            incomplete.push(ObservationIncompleteReason::AccessibilityNode);
        }
        if compact_accessibility.labels_truncated {
            incomplete.push(ObservationIncompleteReason::AccessibilityLabel);
        }
        if compact_accessibility.controls_truncated {
            incomplete.push(ObservationIncompleteReason::Control);
        }
        if page_state.boundaries.child_frames > 0 {
            incomplete.push(ObservationIncompleteReason::FrameBoundary);
        }
        if page_state.boundaries.canvases > 0 {
            incomplete.push(ObservationIncompleteReason::Canvas);
        }
        if page_state.boundaries.truncated {
            incomplete.push(ObservationIncompleteReason::BoundaryScan);
        }
        if !consistent {
            incomplete.push(ObservationIncompleteReason::MutationRace);
        }
        // Shadow piercing: discover which interactive controls are inside open shadow roots.
        let (shadow_paths, pierced_hosts) = if page_state.boundaries.shadow_roots > 0 {
            match self
                .cdp
                .get_flattened_document(crate::browser::dom::MAX_SHADOW_DEPTH as i64)
                .await
            {
                Ok(flattened) => {
                    let flattened = serde_json::to_value(flattened)?;
                    let paths = crate::browser::dom::build_shadow_host_paths(&flattened);
                    let hosts = crate::browser::dom::count_pierced_shadow_hosts(&paths);
                    (paths, hosts)
                }
                Err(_) => (HashMap::new(), 0),
            }
        } else {
            (HashMap::new(), 0)
        };

        // Only flag ShadowBoundary when hosts were not all pierced
        if page_state.boundaries.shadow_roots > 0
            && pierced_hosts < page_state.boundaries.shadow_roots
        {
            incomplete.push(ObservationIncompleteReason::ShadowBoundary);
        }

        // Apply shadow host paths to interactive controls
        if !shadow_paths.is_empty() {
            for control in compact_accessibility.interactive.iter_mut() {
                if let Some(path) = shadow_paths.get(&control.backend_dom_node_id) {
                    control.shadow_host_path = Some(path.clone());
                }
            }
        }

        // Read form field values when explicitly requested
        if include_form_values {
            self.read_form_field_values(&mut compact_accessibility.interactive)
                .await?;
        }

        let interactive_len = compact_accessibility.interactive.len();
        let accessibility = CompactAccessibilitySnapshot {
            page: page.clone(),
            revision: end_revision,
            roots: compact_accessibility.roots,
            interactive: compact_accessibility.interactive,
            truncated: compact_accessibility.truncated,
            omitted_count: compact_accessibility.omitted_count,
            ranking_applied: compact_accessibility.ranking_applied,
            completeness: Some(ObservationCompleteness::compute(
                compact_accessibility.interactive_discovered,
                interactive_len,
                page_state.boundaries.shadow_roots,
                pierced_hosts,
                page_state.boundaries.canvases,
                page_state.boundaries.child_frames,
                !consistent,
            )),
        };
        let context = CompactPageContext {
            page,
            text,
            accessibility,
            consistency: ObservationConsistency {
                consistent,
                attempts,
                start_revision,
                end_revision,
                start_mutation_revision: start_state.mutation_revision,
                end_mutation_revision: page_state.mutation_revision,
            },
            boundaries: page_state.boundaries,
            incomplete,
        };
        if consistent && self.page_revision.load(Ordering::Relaxed) == end_revision {
            *self.observation_cache.lock().await = Some(CachedObservation {
                revision: end_revision,
                context: context.clone(),
            });
            self.cache_accessibility_tree(&target_id, &frame_id, end_revision, &accessibility_raw)
                .await;
        }
        Ok(context)
    }

    async fn cached_accessibility_tree(
        &self,
        target_id: &str,
        frame_id: &str,
        revision: u64,
    ) -> BrowserResult<Value> {
        {
            let cache = self.accessibility_cache.lock().await;
            if let Some(cached) = cache.as_ref()
                && cached.revision == revision
                && cached.target_id == target_id
                && cached.frame_id == frame_id
            {
                return Ok(cached.tree.clone());
            }
        }

        Ok(serde_json::to_value(
            self.cdp.get_accessibility_tree().await?,
        )?)
    }

    async fn cache_accessibility_tree(
        &self,
        target_id: &str,
        frame_id: &str,
        revision: u64,
        tree: &Value,
    ) {
        if serde_json::to_vec(&tree)
            .map(|serialized| serialized.len() <= COMPACT_ACCESSIBILITY_CACHE_MAX_BYTES)
            .unwrap_or(false)
        {
            *self.accessibility_cache.lock().await = Some(CachedAccessibilityTree {
                target_id: target_id.to_string(),
                frame_id: frame_id.to_string(),
                revision,
                tree: tree.clone(),
            });
        }
    }

    /// Read current values of form controls and populate CompactInteractiveElement fields.
    /// Enforces ReadFormValues policy, max 16 fields, password/CC redaction.
    async fn read_form_field_values(
        &self,
        controls: &mut [CompactInteractiveElement],
    ) -> BrowserResult<()> {
        use crate::browser::dom::{
            FORM_VALUE_MAX_BYTES, FORM_VALUE_MAX_FIELDS, SELECT_OPTION_MAX_BYTES, truncate_utf8,
        };

        self.policy.require(PolicyCapability::ReadFormValues)?;
        let allow_sensitive = self.policy.allow_sensitive_form_values();

        const FORM_ROLES: &[&str] = &[
            "textbox",
            "searchbox",
            "combobox",
            "spinbutton",
            "listbox",
            "checkbox",
            "radio",
            "switch",
            "slider",
        ];

        // Prioritize controls with backend node IDs and form-relevant roles
        let mut candidates: Vec<&mut CompactInteractiveElement> = controls
            .iter_mut()
            .filter(|c| {
                FORM_ROLES.iter().any(|r| c.role.eq_ignore_ascii_case(r))
                    && c.backend_dom_node_id > 0
            })
            .take(FORM_VALUE_MAX_FIELDS)
            .collect();

        if candidates.is_empty() {
            return Ok(());
        }

        // Read values via CDP: resolve backend node IDs → object IDs → call function
        let expression = r#"function() {
            const el = this;
            const result = { empty: true };
            const tag = (el.tagName || '').toLowerCase();
            if (tag === 'input') {
                const type = (el.type || 'text').toLowerCase();
                if (type === 'checkbox' || type === 'radio') {
                    result.checked = el.checked;
                    result.value = el.value;
                } else {
                    result.value = el.value;
                }
            } else if (tag === 'select') {
                const opt = el.options[el.selectedIndex];
                result.selectedOption = opt ? (opt.label || opt.text || opt.value) : '';
                result.value = el.value;
            } else if (tag === 'textarea') {
                result.value = el.value;
            } else {
                result.value = el.value || el.textContent || '';
            }
            result.empty = !result.value && !result.selectedOption && !result.checked;
            result.readOnly = !!el.readOnly;
            result.required = !!el.required;
            result.autocomplete = el.getAttribute('autocomplete') || '';
            result.inputType = (el.type || '').toLowerCase();
            return JSON.stringify(result);
        }"#;

        for control in candidates.iter_mut() {
            let resolved = match self
                .cdp
                .send(
                    "DOM.resolveNode",
                    Some(serde_json::json!({
                        "backendNodeId": control.backend_dom_node_id,
                    })),
                )
                .await
            {
                Ok(resolved) => resolved,
                Err(_) => continue,
            };

            let Some(object_id) = resolved["object"]["objectId"].as_str() else {
                continue;
            };
            let remote = RemoteObjectGuard::new(self.cdp.clone(), object_id.to_string());

            let raw_result = self
                .cdp
                .send(
                    "Runtime.callFunctionOn",
                    Some(serde_json::json!({
                        "objectId": &remote.object_id,
                        "functionDeclaration": expression,
                        "returnByValue": true,
                        "awaitPromise": false,
                    })),
                )
                .await;
            let raw = match raw_result {
                Ok(raw) => raw,
                Err(_) => continue,
            };

            let value_str = raw["result"]["value"].as_str().unwrap_or("{}");
            let parsed: Value = match serde_json::from_str(value_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let input_type = parsed["inputType"]
                .as_str()
                .map(String::from)
                .or_else(|| control.input_type.clone());

            let is_password = input_type.as_deref() == Some("password");
            let is_sensitive_autocomplete = parsed["autocomplete"]
                .as_str()
                .map(|ac| ac.starts_with("cc-") || ac == "current-password" || ac == "new-password")
                .unwrap_or(false);

            if let Some(val) = parsed["value"].as_str() {
                if (is_password || is_sensitive_autocomplete) && !allow_sensitive {
                    control.value = Some("<redacted>".to_string());
                } else {
                    let (truncated, _) = truncate_utf8(val, FORM_VALUE_MAX_BYTES);
                    control.value = Some(truncated.to_string());
                }
            }

            if let Some(checked) = parsed["checked"].as_bool() {
                control.checked = Some(checked);
            }

            if let Some(opt) = parsed["selectedOption"].as_str() {
                let (truncated, _) = truncate_utf8(opt, SELECT_OPTION_MAX_BYTES);
                control.selected_option = Some(truncated.to_string());
            }

            control.empty = parsed["empty"].as_bool().unwrap_or(true);
            control.read_only = parsed["readOnly"].as_bool().unwrap_or(false);
            control.required = parsed["required"].as_bool().unwrap_or(false);

            if let Some(it) = input_type {
                control.input_type = Some(it);
            }
        }

        Ok(())
    }

    async fn compact_page_state(&self, context_id: i64) -> BrowserResult<EvaluatedPageState> {
        let raw = self
            .cdp
            .evaluate_in_context(COMPACT_PAGE_STATE_EXPRESSION, Some(context_id))
            .await?;
        Ok(serde_json::from_value(runtime_value(&raw)?)?)
    }

    pub(crate) async fn current_page_context_id(&self) -> BrowserResult<String> {
        let (target_id, frame_id) = self.route_identity().await?;
        for attempt in 0..2 {
            let context_id = self.observation_context_id(&target_id, &frame_id).await?;
            match self.compact_page_state(context_id).await {
                Ok(state)
                    if !state.page_context_id.is_empty()
                        && state.page_context_id.len() <= 128
                        && state.page_context_id.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
                        }) =>
                {
                    return Ok(state.page_context_id);
                }
                Ok(_) => return Err("browser returned an invalid page context identity".into()),
                Err(error) if attempt == 0 && is_stale_observation_context(error.as_ref()) => {
                    self.discard_observation_context(&target_id, &frame_id, context_id)
                        .await;
                }
                Err(error) => return Err(error),
            }
        }
        Err("browser page context identity could not be read".into())
    }

    async fn observation_context_id(&self, target_id: &str, frame_id: &str) -> BrowserResult<i64> {
        let session_id = self.cdp.current_session_id();
        {
            let context = self.observation_context.lock().await;
            if let Some(cached) = context.as_ref()
                && cached.target_id == target_id
                && cached.session_id == session_id
                && cached.frame_id == frame_id
            {
                return Ok(cached.context_id);
            }
        }

        let world = self
            .cdp
            .send(
                "Page.createIsolatedWorld",
                Some(serde_json::json!({"frameId": frame_id, "worldName": "glass-observation"})),
            )
            .await?;
        let context_id = world["executionContextId"]
            .as_i64()
            .ok_or("Page.createIsolatedWorld returned no executionContextId")?;
        *self.observation_context.lock().await = Some(CachedObservationContext {
            target_id: target_id.to_string(),
            session_id,
            frame_id: frame_id.to_string(),
            context_id,
        });
        Ok(context_id)
    }

    async fn discard_observation_context(&self, target_id: &str, frame_id: &str, context_id: i64) {
        let mut context = self.observation_context.lock().await;
        if context.as_ref().is_some_and(|cached| {
            cached.target_id == target_id
                && cached.frame_id == frame_id
                && cached.context_id == context_id
        }) {
            context.take();
        }
    }
}

fn is_stale_observation_context(error: &(dyn std::error::Error + 'static)) -> bool {
    error
        .downcast_ref::<crate::browser::cdp::CdpError>()
        .is_some_and(|error| {
            error.message.contains("Cannot find context")
                || error.message.contains("Execution context was destroyed")
        })
}

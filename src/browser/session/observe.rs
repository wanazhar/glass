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
                let raw = self.cdp.get_deep_document().await?;
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
        self.cdp
            .with_current_route(async {
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
            })
            .await
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
        let mut collected = None;
        for attempt in 1..=COMPACT_OBSERVATION_MAX_ATTEMPTS {
            let start_revision = self.page_revision.load(Ordering::Relaxed);
            let attempt_result = tokio::time::timeout(COMPACT_OBSERVATION_ATTEMPT_TIMEOUT, async {
                let start = self.compact_page_state(context_id).await?;
                let accessibility = self.cdp.get_accessibility_tree().await?;
                let end = self.compact_page_state(context_id).await?;
                BrowserResult::Ok((start, accessibility, end))
            })
            .await
            .map_err(|_| "compact observation attempt exceeded its one-second deadline")??;
            let end_revision = self.page_revision.load(Ordering::Relaxed);
            let consistent = start_revision == end_revision
                && attempt_result.0.mutation_revision == attempt_result.2.mutation_revision;
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
            target_id,
            frame_id,
        };
        let full_roots = parse_accessibility_tree(&accessibility_raw);
        let mut compact_accessibility =
            crate::browser::dom::project_compact_accessibility_with_ranking(
                &full_roots,
                end_revision,
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
        }
        Ok(context)
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

            let raw_result = self
                .cdp
                .send(
                    "Runtime.callFunctionOn",
                    Some(serde_json::json!({
                        "objectId": object_id,
                        "functionDeclaration": expression,
                        "returnByValue": true,
                        "awaitPromise": false,
                    })),
                )
                .await;
            // DOM.resolveNode creates a remote object even when the following
            // call fails. Release it on every path; form snapshots are bounded
            // but must not turn each observe into a remote-handle leak.
            let _ = self.cdp.release_object(object_id).await;
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
        let value = runtime_value(&raw)?;
        let json = value
            .as_str()
            .ok_or("compact page-state evaluation returned a non-string value")?;
        Ok(serde_json::from_str(json)?)
    }
}

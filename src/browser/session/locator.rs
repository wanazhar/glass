//! Element resolution with fallback chains.
//!
//! Resolves locator strings (ref, accessible name, role+name, text, CSS,
//! ordinal) into specific DOM elements. Supports fallback chains of up to
//! [`MAX_FALLBACK_SEGMENTS`] locator segments.

use super::*;

#[derive(Debug)]
struct PreflightProbeError {
    reason: TargetActionabilityReason,
    geometry: Option<PreflightGeometry>,
    hints: PreflightHints,
    diagnostics: Option<TargetDiagnostics>,
}

impl std::fmt::Display for PreflightProbeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "preflight probe failed: {:?}", self.reason)
    }
}

impl std::error::Error for PreflightProbeError {}

/// Maximum number of locator segments in a fallback chain.
const MAX_FALLBACK_SEGMENTS: usize = 8;
/// Maximum UTF-8 byte length of a single locator segment.
const MAX_SEGMENT_BYTES: usize = 1024;

impl BrowserSession {
    pub(crate) async fn resolve_element(&self, target: &str) -> BrowserResult<ResolvedElement> {
        // Fallback chain: split on " | " (pipe with surrounding spaces)
        if let Some(_pipe_pos) = target.find(" | ") {
            let segments: Vec<&str> = target.split(" | ").collect();
            if segments.len() > MAX_FALLBACK_SEGMENTS {
                return Err(format!(
                    "locator fallback chain exceeds max {} segments",
                    MAX_FALLBACK_SEGMENTS
                )
                .into());
            }
            for segment in &segments {
                if segment.len() > MAX_SEGMENT_BYTES {
                    let preview_end = segment
                        .char_indices()
                        .take_while(|(index, _)| *index < 80)
                        .map(|(index, ch)| index + ch.len_utf8())
                        .last()
                        .unwrap_or(0)
                        .min(segment.len());
                    return Err(format!(
                        "locator segment exceeds {} bytes: {}",
                        MAX_SEGMENT_BYTES,
                        &segment[..preview_end]
                    )
                    .into());
                }
            }

            for segment in &segments {
                let locator = Locator::parse(segment)?;
                match self.resolve_locator(&locator).await? {
                    TargetResolution::Unique(element) => return Ok(element),
                    TargetResolution::Ambiguous(candidates) => {
                        // Stop immediately on ambiguity — never try next segment
                        return Err(TargetError {
                            kind: TargetErrorKind::Ambiguous,
                            reason: None,
                            candidates,
                            recovery: None,
                            diagnostics: None,
                        }
                        .into());
                    }
                    TargetResolution::NotFound => {
                        // Continue to next segment
                    }
                }
            }

            // All segments exhausted without Unique match
            Err(TargetError {
                kind: TargetErrorKind::NotFound,
                reason: None,
                candidates: Vec::new(),
                recovery: None,
                diagnostics: None,
            }
            .into())
        } else {
            // Single locator — no behavioral change
            let locator = Locator::parse(target)?;
            match self.resolve_locator(&locator).await? {
                TargetResolution::Unique(element) => Ok(element),
                TargetResolution::Ambiguous(candidates) => Err(TargetError {
                    kind: TargetErrorKind::Ambiguous,
                    reason: None,
                    candidates,
                    recovery: None,
                    diagnostics: None,
                }
                .into()),
                TargetResolution::NotFound => Err(TargetError {
                    kind: TargetErrorKind::NotFound,
                    reason: None,
                    candidates: Vec::new(),
                    recovery: None,
                    diagnostics: None,
                }
                .into()),
            }
        }
    }

    pub async fn preflight(&self, target: &str) -> PreflightOutcome {
        self.preflight_with_action(target, PreflightAction::Click)
            .await
    }

    pub async fn preflight_with_action(
        &self,
        target: &str,
        action: PreflightAction,
    ) -> PreflightOutcome {
        let revision = self.page_revision.load(Ordering::Relaxed);
        let (target_id, frame_id) = self
            .route_identity()
            .await
            .ok()
            .map(|(target, frame)| (Some(target), Some(frame)))
            .unwrap_or((None, None));

        let element = match self.resolve_element(target).await {
            Ok(element) => element,
            Err(error) => {
                let error_kind = error.downcast_ref::<TargetError>().map(|e| e.kind);
                return PreflightOutcome {
                    action,
                    unique: false,
                    element: None,
                    actionable: None,
                    actionability_reason: None,
                    candidates: error
                        .downcast_ref::<TargetError>()
                        .map(|e| e.candidates.clone())
                        .unwrap_or_default(),
                    error_kind,
                    diagnostics: error
                        .downcast_ref::<PreflightProbeError>()
                        .and_then(|e| e.diagnostics.clone()),
                    revision,
                    geometry: None,
                    hints: PreflightHints::default(),
                    target_id,
                    frame_id,
                };
            }
        };

        // Use a distinct read-only probe. The normal action probe may scroll
        // the target into view, which is correct before a click but violates
        // preflight's side-effect-free contract.
        let (actionable, actionability_reason, geometry, hints, diagnostics) =
            match self.check_element_preflight(&element).await {
                Ok((geometry, hints)) => (true, None, Some(geometry), hints, None),
                Err(error) => {
                    let target_error = error.downcast_ref::<TargetError>();
                    let probe_error = error.downcast_ref::<PreflightProbeError>();
                    let reason = probe_error
                        .map(|e| e.reason)
                        .or_else(|| target_error.and_then(|e| e.reason));
                    let geometry = probe_error.and_then(|e| e.geometry);
                    let hints = probe_error.map(|e| e.hints).unwrap_or_default();
                    let diagnostics = probe_error.and_then(|e| e.diagnostics.clone());
                    (false, reason, geometry, hints, diagnostics)
                }
            };

        PreflightOutcome {
            action,
            unique: true,
            element: Some(element),
            actionable: Some(actionable),
            actionability_reason,
            candidates: Vec::new(),
            error_kind: None,
            revision,
            geometry,
            hints,
            diagnostics,
            target_id,
            frame_id,
        }
    }

    async fn check_element_preflight(
        &self,
        element: &ResolvedElement,
    ) -> BrowserResult<(PreflightGeometry, PreflightHints)> {
        let backend_node_id = element
            .backend_dom_node_id
            .ok_or("element has no backend node id")?;
        let object_id = self
            .cdp
            .resolve_node_object(None, Some(backend_node_id))
            .await
            .map_err(|_| PreflightProbeError {
                reason: TargetActionabilityReason::NodeUnavailable,
                geometry: None,
                hints: PreflightHints::default(),
                diagnostics: None,
            })?;
        let remote = RemoteObjectGuard::new(self.cdp.clone(), object_id);
        let raw = self
            .cdp
            .call_on_object(&remote.object_id, PREFLIGHT_FUNCTION)
            .await
            .map_err(|_| PreflightProbeError {
                reason: TargetActionabilityReason::NodeUnavailable,
                geometry: None,
                hints: PreflightHints::default(),
                diagnostics: None,
            })?;
        let value = runtime_value(&raw)?;
        let geometry = value["geometry"].as_object().and_then(|geometry| {
            Some(PreflightGeometry {
                x: geometry.get("x")?.as_f64()?,
                y: geometry.get("y")?.as_f64()?,
                width: geometry.get("width")?.as_f64()?,
                height: geometry.get("height")?.as_f64()?,
            })
        });
        let hints = PreflightHints {
            likely_navigation: value["hints"]["likelyNavigation"]
                .as_bool()
                .unwrap_or(false),
            likely_popup: value["hints"]["likelyPopup"].as_bool().unwrap_or(false),
            likely_form_submit: value["hints"]["likelyFormSubmit"]
                .as_bool()
                .unwrap_or(false),
        };
        if value["ok"].as_bool() != Some(true) {
            let reason = value["reason"].as_str().unwrap_or("verification_failed");
            return Err(PreflightProbeError {
                reason: actionability_reason(reason),
                geometry,
                hints,
                diagnostics: serde_json::from_value(value["diagnostics"].clone()).ok(),
            }
            .into());
        }
        Ok((geometry.ok_or("preflight returned no geometry")?, hints))
    }

    pub(crate) async fn resolve_locator(
        &self,
        locator: &Locator,
    ) -> BrowserResult<TargetResolution> {
        if let Locator::Reference(target) = locator {
            let reference = parse_revisioned_reference(target)?
                .ok_or_else(|| format!("invalid revisioned element reference: {target}"))?;
            let current_context_id = self.page_context_id().await?;
            if reference.context_id.as_deref() != Some(current_context_id.as_str()) {
                return Err(TargetError {
                    kind: TargetErrorKind::StaleReference,
                    reason: None,
                    candidates: Vec::new(),
                    recovery: Some(StaleReferenceRecovery {
                        suggestion: "reobserve",
                        from_revision: reference.revision,
                        stale_ref: target.to_string(),
                    }),
                    diagnostics: None,
                }
                .into());
            }
            let current_revision = self.page_revision.load(Ordering::Relaxed);
            if reference.revision != current_revision {
                return Err(TargetError {
                    kind: TargetErrorKind::StaleReference,
                    reason: None,
                    candidates: Vec::new(),
                    recovery: Some(StaleReferenceRecovery {
                        suggestion: "reconcileReferences",
                        from_revision: reference.revision,
                        stale_ref: target.to_string(),
                    }),
                    diagnostics: None,
                }
                .into());
            }
            return Ok(TargetResolution::Unique(ResolvedElement {
                node_id: None,
                backend_dom_node_id: Some(reference.backend_dom_node_id),
                label: target.to_string(),
                reference: Some(target.to_string()),
                role: None,
                input_type: None,
            }));
        }

        let revision = self.page_revision.load(Ordering::Relaxed);
        let raw = serde_json::to_value(self.cdp.get_accessibility_tree().await?)?;
        let roots = parse_accessibility_tree(&raw);
        let context_id = self.page_context_id().await?;
        let interactive = interactive_elements_with_context(&roots, revision, Some(&context_id));
        let matches: Vec<&InteractiveElement> = match locator {
            Locator::AccessibleName(name) => interactive
                .iter()
                .filter(|element| element.name.eq_ignore_ascii_case(name))
                .take(AMBIGUOUS_CANDIDATE_LIMIT + 1)
                .collect(),
            Locator::RoleAndName { role, name } => interactive
                .iter()
                .filter(|element| {
                    element.role.eq_ignore_ascii_case(role)
                        && element.name.eq_ignore_ascii_case(name)
                })
                .take(AMBIGUOUS_CANDIDATE_LIMIT + 1)
                .collect(),
            Locator::Ordinal(index) => interactive.get(index - 1).into_iter().collect(),
            Locator::Reference(_) | Locator::Text(_) | Locator::Css(_) => Vec::new(),
        };
        if matches.len() == 1 {
            let element = matches[0];
            return Ok(TargetResolution::Unique(ResolvedElement {
                node_id: None,
                backend_dom_node_id: Some(element.backend_dom_node_id),
                label: format!("{} {}", element.role, element.name),
                reference: Some(element.reference.clone()),
                role: Some(element.role.clone()),
                input_type: element.input_type.clone(),
            }));
        }
        if matches.len() > 1 {
            return Ok(TargetResolution::Ambiguous(
                matches
                    .into_iter()
                    .take(AMBIGUOUS_CANDIDATE_LIMIT)
                    .map(|element| CandidateSummary {
                        label: bounded_candidate_label(&format!(
                            "{} {}",
                            element.role, element.name
                        )),
                        reference: Some(element.reference.clone()),
                    })
                    .collect(),
            ));
        }

        match locator {
            Locator::Css(selector) => {
                let expression = css_query_expression(selector)?;
                let (count, nodes) = self
                    .cdp
                    .bounded_element_query(&expression, AMBIGUOUS_CANDIDATE_LIMIT)
                    .await?;
                dom_nodes_resolution(count, nodes, format!("css={selector}"), "css match")
            }
            Locator::Text(text) => {
                let expression = text_query_expression(text)?;
                let (count, nodes) = self
                    .cdp
                    .bounded_element_query(&expression, AMBIGUOUS_CANDIDATE_LIMIT)
                    .await?;
                if count > 1 {
                    return Ok(TargetResolution::Ambiguous(
                        (1..=count.min(AMBIGUOUS_CANDIDATE_LIMIT))
                            .map(|index| CandidateSummary {
                                label: format!("text match {index}"),
                                reference: None,
                            })
                            .collect(),
                    ));
                }
                dom_nodes_resolution(count, nodes, format!("text={text}"), "text match")
            }
            Locator::Reference(_)
            | Locator::AccessibleName(_)
            | Locator::RoleAndName { .. }
            | Locator::Ordinal(_) => Ok(TargetResolution::NotFound),
        }
    }
}

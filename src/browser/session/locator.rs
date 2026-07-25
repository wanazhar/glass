use super::*;

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
                    return Err(format!(
                        "locator segment exceeds {} bytes: {}",
                        MAX_SEGMENT_BYTES,
                        &segment[..segment.len().min(80)]
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
                }
                .into()),
                TargetResolution::NotFound => Err(TargetError {
                    kind: TargetErrorKind::NotFound,
                    reason: None,
                    candidates: Vec::new(),
                }
                .into()),
            }
        }
    }

    pub async fn preflight(&self, target: &str) -> PreflightOutcome {
        let revision = self.page_revision.load(Ordering::Relaxed);

        let element = match self.resolve_element(target).await {
            Ok(element) => element,
            Err(error) => {
                let error_kind = error.downcast_ref::<TargetError>().map(|e| e.kind);
                return PreflightOutcome {
                    unique: false,
                    element: None,
                    actionable: None,
                    actionability_reason: None,
                    candidates: error
                        .downcast_ref::<TargetError>()
                        .map(|e| e.candidates.clone())
                        .unwrap_or_default(),
                    error_kind,
                    revision,
                };
            }
        };

        // Try actionability check (side-effect-free where possible)
        let (actionable, actionability_reason) =
            match self.check_element_actionability(&element).await {
                Ok(()) => (true, None),
                Err(error) => {
                    let reason = error.downcast_ref::<TargetError>().and_then(|e| e.reason);
                    (false, reason)
                }
            };

        PreflightOutcome {
            unique: true,
            element: Some(element),
            actionable: Some(actionable),
            actionability_reason,
            candidates: Vec::new(),
            error_kind: None,
            revision,
        }
    }

    /// Check whether a resolved element is actionable without performing the action.
    async fn check_element_actionability(&self, element: &ResolvedElement) -> BrowserResult<()> {
        let backend_node_id = element
            .backend_dom_node_id
            .ok_or_else(|| "element has no backend node id")?;

        let object_id = self
            .cdp
            .resolve_node_object(None, Some(backend_node_id))
            .await
            .map_err(|_| TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(TargetActionabilityReason::NodeUnavailable),
                candidates: Vec::new(),
            })?;

        let raw = self
            .cdp
            .call_on_object(&object_id, HIT_TEST_FUNCTION)
            .await
            .map_err(|_| TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(TargetActionabilityReason::NodeUnavailable),
                candidates: Vec::new(),
            })?;

        let value = runtime_value(&raw)?;
        if value["ok"].as_bool() != Some(true) {
            let reason = value["reason"].as_str().unwrap_or("verification_failed");
            return Err(TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(actionability_reason(reason)),
                candidates: Vec::new(),
            }
            .into());
        }

        Ok(())
    }

    pub(crate) async fn resolve_locator(
        &self,
        locator: &Locator,
    ) -> BrowserResult<TargetResolution> {
        if let Locator::Reference(target) = locator {
            let reference = parse_revisioned_reference(target)?
                .ok_or_else(|| format!("invalid revisioned element reference: {target}"))?;
            let current_revision = self.page_revision.load(Ordering::Relaxed);
            if reference.revision != current_revision {
                return Err(TargetError {
                    kind: TargetErrorKind::StaleReference,
                    reason: None,
                    candidates: Vec::new(),
                }
                .into());
            }
            return Ok(TargetResolution::Unique(ResolvedElement {
                node_id: None,
                backend_dom_node_id: Some(reference.backend_dom_node_id),
                label: target.to_string(),
                reference: Some(target.to_string()),
            }));
        }

        let snapshot = self.snapshot().await?;
        let matches: Vec<&InteractiveElement> = match locator {
            Locator::AccessibleName(name) => snapshot
                .interactive
                .iter()
                .filter(|element| element.name.eq_ignore_ascii_case(name))
                .take(AMBIGUOUS_CANDIDATE_LIMIT + 1)
                .collect(),
            Locator::RoleAndName { role, name } => snapshot
                .interactive
                .iter()
                .filter(|element| {
                    element.role.eq_ignore_ascii_case(role)
                        && element.name.eq_ignore_ascii_case(name)
                })
                .take(AMBIGUOUS_CANDIDATE_LIMIT + 1)
                .collect(),
            Locator::Ordinal(index) => snapshot.interactive.get(index - 1).into_iter().collect(),
            Locator::Reference(_) | Locator::Text(_) | Locator::Css(_) => Vec::new(),
        };
        if matches.len() == 1 {
            let element = matches[0];
            return Ok(TargetResolution::Unique(ResolvedElement {
                node_id: None,
                backend_dom_node_id: Some(element.backend_dom_node_id),
                label: format!("{} {}", element.role, element.name),
                reference: Some(element.reference.clone()),
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

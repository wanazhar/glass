use super::*;

impl BrowserSession {
    pub(crate) async fn resolve_element(&self, target: &str) -> BrowserResult<ResolvedElement> {
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

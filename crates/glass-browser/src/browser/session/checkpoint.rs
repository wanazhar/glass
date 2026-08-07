//! Cross-process session checkpoint export/import.
//!
//! Serializes a compact [`CheckpointV1`] containing the current URL, title,
//! frame tree topology, and revision-tagged element references. Checkpoints
//! are ≤ 4 KiB and designed for cross-process resume.

use super::*;

impl BrowserSession {
    /// Return a bounded delta between the cached compact observation and a
    /// fresh observation on the same route. Historical snapshots are not
    /// retained beyond this request.
    pub async fn observe_delta(&self) -> BrowserResult<ObservationDelta> {
        let previous = self
            .observation_cache
            .lock()
            .await
            .as_ref()
            .map(|cached| cached.context.clone())
            .ok_or("observe_delta requires a prior compact observation")?;
        let current = self.observe_fresh().await?;
        if previous.page.target_id != current.page.target_id
            || previous.page.frame_id != current.page.frame_id
        {
            return Err("observe_delta cannot compare different routes".into());
        }

        let old_by_backend: HashMap<_, _> = previous
            .accessibility
            .interactive
            .iter()
            .map(|control| (control.backend_dom_node_id, control))
            .collect();
        let new_by_backend: HashMap<_, _> = current
            .accessibility
            .interactive
            .iter()
            .map(|control| (control.backend_dom_node_id, control))
            .collect();
        let control = |value: &CompactInteractiveElement| DeltaControl {
            reference: value.reference.clone(),
            role: value.role.clone(),
            name: value.name.clone(),
        };
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut changed = Vec::new();
        for value in &current.accessibility.interactive {
            match old_by_backend.get(&value.backend_dom_node_id) {
                None if added.len() < 8 => added.push(control(value)),
                Some(old)
                    if (old.role != value.role || old.name != value.name) && changed.len() < 8 =>
                {
                    changed.push(control(value));
                }
                _ => {}
            }
        }
        for value in &previous.accessibility.interactive {
            if !new_by_backend.contains_key(&value.backend_dom_node_id) && removed.len() < 8 {
                removed.push(control(value));
            }
        }
        let revision_delta = current
            .accessibility
            .revision
            .saturating_sub(previous.accessibility.revision);
        Ok(ObservationDelta {
            from_revision: previous.accessibility.revision,
            to_revision: current.accessibility.revision,
            mutation_summary: MutationSummary {
                url_changed: previous.page.url != current.page.url,
                title_changed: previous.page.title != current.page.title,
                revision_delta,
                soft_navigation_suspected: revision_delta > 0
                    && previous.page.url == current.page.url
                    && previous.page.title == current.page.title,
            },
            added,
            removed,
            changed,
            prior_incomplete: previous.incomplete,
            current_incomplete: current.incomplete,
        })
    }

    /// Reconcile prior references against the current page revision.
    /// Maps old refs (`r<fromRevision>:b<id>`) to current refs via backend
    /// node identity or stable role+name matching.
    pub async fn reconcile_references(
        &self,
        from_revision: u64,
        refs: &[String],
    ) -> BrowserResult<ReconciliationOutcome> {
        self.reconcile_references_with_options(
            from_revision,
            refs,
            &ReconciliationOptions::default(),
        )
        .await
    }

    /// Reconcile revisioned references with bounded stable hints and an
    /// optional scope. Hints are only used after backend identity and the
    /// prior role/name identity fail; ambiguity always remains a loss.
    pub async fn reconcile_references_with_options(
        &self,
        from_revision: u64,
        refs: &[String],
        options: &ReconciliationOptions,
    ) -> BrowserResult<ReconciliationOutcome> {
        if refs.len() > MAX_RECONCILE_REFS {
            return Err(format!(
                "too many refs to reconcile: {} (max {})",
                refs.len(),
                MAX_RECONCILE_REFS
            )
            .into());
        }
        if options.hints.len() > MAX_RECONCILE_HINTS {
            return Err(format!(
                "too many reconciliation hints: {} (max {})",
                options.hints.len(),
                MAX_RECONCILE_HINTS
            )
            .into());
        }

        let current_revision = self.page_revision.load(Ordering::Relaxed);
        let prior_page = self
            .observation_cache
            .lock()
            .await
            .as_ref()
            .filter(|cached| cached.revision == from_revision)
            .map(|cached| cached.context.page.clone());
        let prior = {
            let cache = self.observation_cache.lock().await;
            cache
                .as_ref()
                .filter(|cached| cached.revision == from_revision)
                .map(|cached| {
                    (
                        cached.context.page.target_id.clone(),
                        cached.context.page.frame_id.clone(),
                        cached
                            .context
                            .accessibility
                            .interactive
                            .iter()
                            .map(|control| {
                                (
                                    control.reference.clone(),
                                    control.role.clone(),
                                    control.name.clone(),
                                    control.backend_dom_node_id,
                                    control.ancestor_path.clone(),
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                })
        };
        if current_revision == from_revision {
            if prior.is_none() {
                return bounded_reconciliation_outcome(ReconciliationOutcome {
                    status: ReconciliationStatus::Complete,
                    to_revision: current_revision,
                    preserved: 0,
                    relocated: 0,
                    lost: refs.len(),
                    mappings: refs
                        .iter()
                        .map(|old| ReferenceMapping::Lost {
                            old: old.clone(),
                            reason: ReferenceLostReason::StaleBoundary,
                        })
                        .collect(),
                    mutation_summary: MutationSummary::default(),
                    incomplete: vec![ObservationIncompleteReason::BoundaryScan],
                });
            }
            // Same revision: validate the ref shape and revision before
            // claiming preservation. A malformed or foreign-revision ref is
            // never silently accepted.
            let known_refs: HashSet<&str> = prior
                .as_ref()
                .map(|(_, _, controls)| {
                    controls
                        .iter()
                        .map(|(reference, _, _, _, _)| reference.as_str())
                        .collect()
                })
                .unwrap_or_default();
            let scope_label = options.scope_ref.as_ref().and_then(|scope_ref| {
                parse_revisioned_reference(scope_ref)
                    .ok()
                    .flatten()
                    .filter(|scope| scope.revision == from_revision)
                    .and_then(|scope| {
                        prior.as_ref().and_then(|(_, _, controls)| {
                            controls
                                .iter()
                                .find(|(_, _, _, backend, _)| *backend == scope.backend_dom_node_id)
                                .map(|(_, role, name, _, _)| format!("{}:{}", role, name))
                        })
                    })
            });
            let scope_invalid = options.scope_ref.is_some() && scope_label.is_none();
            let mappings: Vec<_> = refs
                .iter()
                .map(|old| {
                    let valid = matches!(
                        parse_revisioned_reference(old),
                        Ok(Some(reference))
                            if reference.revision == from_revision
                                && known_refs.contains(old.as_str())
                    );
                    if !valid {
                        return ReferenceMapping::Lost {
                            old: old.clone(),
                            reason: ReferenceLostReason::StaleBoundary,
                        };
                    }
                    if scope_invalid
                        || scope_label.as_deref().is_some_and(|scope| {
                            !prior.as_ref().is_some_and(|(_, _, controls)| {
                                controls.iter().any(|(reference, _, _, _, ancestors)| {
                                    reference == old
                                        && ancestors.iter().any(|ancestor| ancestor == scope)
                                })
                            })
                        })
                    {
                        return ReferenceMapping::Lost {
                            old: old.clone(),
                            reason: ReferenceLostReason::OutOfScope,
                        };
                    }
                    ReferenceMapping::Preserved {
                        old: old.clone(),
                        new: old.clone(),
                    }
                })
                .collect();
            let preserved = mappings
                .iter()
                .filter(|mapping| matches!(mapping, ReferenceMapping::Preserved { .. }))
                .count();
            let lost = mappings.len() - preserved;
            return bounded_reconciliation_outcome(ReconciliationOutcome {
                status: ReconciliationStatus::Complete,
                to_revision: current_revision,
                preserved,
                relocated: 0,
                lost,
                mappings,
                mutation_summary: MutationSummary::default(),
                incomplete: Vec::new(),
            });
        }

        // Get fresh compact observe to see current controls
        let context = self.observe_fresh().await?;
        let to_revision = context.accessibility.revision;

        // Build lookup: backend_dom_node_id -> current ref
        let mut current_by_backend: HashMap<i64, String> = HashMap::new();
        let mut current_controls: Vec<(&str, &str, i64, &str, &[String])> = Vec::new();
        let ax = &context.accessibility;
        for c in &ax.interactive {
            current_by_backend.insert(c.backend_dom_node_id, c.reference.clone());
            current_controls.push((
                &c.role,
                &c.name,
                c.backend_dom_node_id,
                &c.reference,
                &c.ancestor_path,
            ));
        }

        // A cached observation is the only safe source of the old route and
        // stable identity. If it is unavailable (for example after a process
        // restart), do not guess relocation from a raw backend ID.
        let route_changed = prior.as_ref().is_none_or(|(old_target, old_frame, _)| {
            context.page.target_id != *old_target || context.page.frame_id != *old_frame
        });

        let scope_label = options.scope_ref.as_ref().and_then(|scope_ref| {
            parse_revisioned_reference(scope_ref)
                .ok()
                .flatten()
                .filter(|scope| scope.revision == from_revision)
                .and_then(|scope| {
                    current_controls
                        .iter()
                        .find(|(_, _, backend, _, _)| *backend == scope.backend_dom_node_id)
                        .map(|(role, name, _, _, _)| format!("{}:{}", role, name))
                })
        });
        let scope_invalid = options.scope_ref.is_some() && scope_label.is_none();

        let mut mappings = Vec::with_capacity(refs.len());
        let mut preserved = 0usize;
        let mut relocated = 0usize;
        let mut lost = 0usize;

        for old_ref in refs {
            // Parse old ref: "r<revision>:b<backend_id>"
            let Ok(Some(reference)) = parse_revisioned_reference(old_ref) else {
                mappings.push(ReferenceMapping::Lost {
                    old: old_ref.clone(),
                    reason: ReferenceLostReason::StaleBoundary,
                });
                lost += 1;
                continue;
            };
            if reference.revision != from_revision || route_changed {
                mappings.push(ReferenceMapping::Lost {
                    old: old_ref.clone(),
                    reason: ReferenceLostReason::StaleBoundary,
                });
                lost += 1;
                continue;
            }
            let backend_id = reference.backend_dom_node_id;

            if scope_invalid {
                mappings.push(ReferenceMapping::Lost {
                    old: old_ref.clone(),
                    reason: ReferenceLostReason::OutOfScope,
                });
                lost += 1;
                continue;
            }

            // Try preserved (same backend node ID), while honoring scope.
            if let Some(new_ref) = current_by_backend.get(&backend_id) {
                if let Some(scope) = scope_label.as_deref()
                    && !current_controls
                        .iter()
                        .find(|control| control.3 == new_ref)
                        .is_some_and(|control| control.4.iter().any(|ancestor| ancestor == scope))
                {
                    mappings.push(ReferenceMapping::Lost {
                        old: old_ref.clone(),
                        reason: ReferenceLostReason::OutOfScope,
                    });
                    lost += 1;
                    continue;
                }
                mappings.push(ReferenceMapping::Preserved {
                    old: old_ref.clone(),
                    new: new_ref.clone(),
                });
                preserved += 1;
                continue;
            }

            // First use the prior control's exact role/name identity.
            let prior_control = prior.as_ref().and_then(|(_, _, controls)| {
                controls
                    .iter()
                    .find(|(reference, _, _, _, _)| reference == old_ref)
            });
            let mut match_kind = ReferenceMatch::RoleAndName;
            let mut matches: Vec<_> = prior_control
                .map(|(_, role, name, _, _)| {
                    current_controls
                        .iter()
                        .filter(|(current_role, current_name, _, _, _)| {
                            current_role.eq_ignore_ascii_case(role)
                                && current_name.eq_ignore_ascii_case(name)
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Stable hints are positional and only run after backend and
            // prior role/name identity fail. This keeps the API bounded and
            // preserves locator-chain ambiguity semantics.
            if matches.is_empty()
                && let Some(hint) = options
                    .hints
                    .get(refs.iter().position(|r| r == old_ref).unwrap_or(usize::MAX))
            {
                match_kind = match hint {
                    Locator::AccessibleName(_) => ReferenceMatch::AccessibleName,
                    _ => ReferenceMatch::Hint,
                };
                matches = current_controls
                    .iter()
                    .filter(|control| locator_matches_compact(hint, control))
                    .collect();
            }

            if let Some(scope) = scope_label.as_deref() {
                matches.retain(|(_, _, _, _, ancestors)| {
                    ancestors.iter().any(|ancestor| ancestor == scope)
                });
                if matches.is_empty() && prior_control.is_some() {
                    mappings.push(ReferenceMapping::Lost {
                        old: old_ref.clone(),
                        reason: ReferenceLostReason::OutOfScope,
                    });
                    lost += 1;
                    continue;
                }
                if !matches.is_empty() {
                    match_kind = ReferenceMatch::ScopedHint;
                }
            }
            if matches.len() == 1 {
                mappings.push(ReferenceMapping::Relocated {
                    old: old_ref.clone(),
                    new: matches[0].3.to_string(),
                    matched_by: match_kind,
                });
                relocated += 1;
                continue;
            }
            if matches.len() > 1 {
                mappings.push(ReferenceMapping::Lost {
                    old: old_ref.clone(),
                    reason: ReferenceLostReason::Ambiguous {
                        candidates: matches
                            .iter()
                            .take(AMBIGUOUS_CANDIDATE_LIMIT)
                            .map(|(role, name, _, reference, _)| CandidateSummary {
                                label: bounded_candidate_label(&format!("{} {}", role, name)),
                                reference: Some((*reference).to_string()),
                            })
                            .collect(),
                    },
                });
                lost += 1;
                continue;
            }
            mappings.push(ReferenceMapping::Lost {
                old: old_ref.clone(),
                reason: ReferenceLostReason::NotFound,
            });
            lost += 1;
        }

        let mutation_summary = prior_page
            .as_ref()
            .map(|prior| MutationSummary {
                url_changed: prior.url != context.page.url,
                title_changed: prior.title != context.page.title,
                revision_delta: to_revision.saturating_sub(from_revision),
                soft_navigation_suspected: to_revision > from_revision
                    && prior.url == context.page.url
                    && prior.title == context.page.title,
            })
            .unwrap_or_default();
        bounded_reconciliation_outcome(ReconciliationOutcome {
            status: if route_changed {
                ReconciliationStatus::RouteChanged
            } else {
                ReconciliationStatus::Complete
            },
            to_revision,
            mappings,
            preserved,
            relocated,
            lost,
            mutation_summary,
            incomplete: context.incomplete,
        })
    }

    /// Export a session checkpoint for cross-process resume.
    /// Returns JSON bounded to ≤ 4 KiB. No cookies, passwords, or form values.
    pub async fn export_checkpoint(&self) -> BrowserResult<CheckpointV1> {
        let page = self.page_info().await?;
        let revision = self.page_revision.load(Ordering::Relaxed);

        let last_refs: Vec<String> = self
            .observation_cache
            .lock()
            .await
            .as_ref()
            .map(|cached| {
                cached
                    .context
                    .accessibility
                    .interactive
                    .iter()
                    .take(8)
                    .map(|c| c.reference.clone())
                    .collect()
            })
            .unwrap_or_default();

        let checkpoint = CheckpointV1 {
            schema_version: 1,
            glass_version: env!("CARGO_PKG_VERSION").to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            profile: self.profile.clone(),
            attach_mode: self.chrome.is_none(),
            topology: CheckpointTopology {
                target_id: Some(page.target_id),
                frame_id: Some(page.frame_id),
                url: bounded_checkpoint_text(&page.url, 1024),
                title: bounded_checkpoint_text(&page.title, 1024),
            },
            observation: CheckpointObservation {
                revision,
                last_refs,
            },
            policy: format!("{:?}", self.policy.preset()).to_lowercase(),
        };
        if serde_json::to_vec(&checkpoint)?.len() > 4 * 1024 {
            return Err("checkpoint exceeds the 4 KiB serialized limit".into());
        }
        Ok(checkpoint)
    }

    /// Import a checkpoint and validate its topology.
    /// Does NOT auto-click — only restores target/frame selection context.
    pub async fn import_checkpoint(&self, checkpoint: &CheckpointV1) -> BrowserResult<()> {
        if checkpoint.schema_version != 1 {
            return Err(CheckpointError::SchemaVersionMismatch {
                expected: 1,
                found: checkpoint.schema_version,
            }
            .into());
        }

        if let Some(ref target_id) = checkpoint.topology.target_id {
            let targets = self.list_targets().await?;
            if !targets.iter().any(|t| t.id == *target_id) {
                return Err(CheckpointError::TargetClosed.into());
            }
            self.select_target(target_id).await?;
        }

        if let Some(ref frame_id) = checkpoint.topology.frame_id {
            let frames = self.list_frames().await?;
            if !frames.iter().any(|f| f.id == *frame_id) {
                return Err(CheckpointError::Stale.into());
            }
            self.select_frame(frame_id).await?;
        }

        Ok(())
    }
}

fn bounded_reconciliation_outcome(
    outcome: ReconciliationOutcome,
) -> BrowserResult<ReconciliationOutcome> {
    let bytes = serde_json::to_vec(&outcome)?.len();
    if bytes > MAX_RECONCILIATION_BYTES {
        return Err(format!(
            "reconciliation response exceeds {} bytes; retry with fewer refs",
            MAX_RECONCILIATION_BYTES
        )
        .into());
    }
    Ok(outcome)
}

fn locator_matches_compact(
    locator: &Locator,
    control: &(&str, &str, i64, &str, &[String]),
) -> bool {
    match locator {
        Locator::AccessibleName(name) => control.1.eq_ignore_ascii_case(name),
        Locator::RoleAndName { role, name } => {
            control.0.eq_ignore_ascii_case(role) && control.1.eq_ignore_ascii_case(name)
        }
        Locator::Text(text) => control.1.eq_ignore_ascii_case(text),
        Locator::Reference(reference) => control.3 == reference,
        Locator::Css(_) | Locator::Ordinal(_) => false,
    }
}

fn bounded_checkpoint_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

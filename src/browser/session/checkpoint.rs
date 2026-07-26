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
        if refs.len() > MAX_RECONCILE_REFS {
            return Err(format!(
                "too many refs to reconcile: {} (max {})",
                refs.len(),
                MAX_RECONCILE_REFS
            )
            .into());
        }

        let current_revision = self.page_revision.load(Ordering::Relaxed);
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
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                })
        };
        if current_revision == from_revision {
            if prior.is_none() {
                return Ok(ReconciliationOutcome {
                    to_revision: current_revision,
                    preserved: 0,
                    relocated: 0,
                    lost: refs.len(),
                    mappings: refs
                        .iter()
                        .map(|old| ReferenceMapping::Lost {
                            old: old.clone(),
                            reason: "missing_prior_snapshot".to_string(),
                        })
                        .collect(),
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
                        .map(|(reference, _, _, _)| reference.as_str())
                        .collect()
                })
                .unwrap_or_default();
            let mappings: Vec<_> = refs
                .iter()
                .map(|old| match parse_revisioned_reference(old) {
                    Ok(Some(reference))
                        if reference.revision == from_revision
                            && known_refs.contains(old.as_str()) =>
                    {
                        ReferenceMapping::Preserved {
                            old: old.clone(),
                            new: old.clone(),
                        }
                    }
                    _ => ReferenceMapping::Lost {
                        old: old.clone(),
                        reason: if known_refs.contains(old.as_str()) {
                            "invalid_ref_format"
                        } else {
                            "not_in_snapshot"
                        }
                        .to_string(),
                    },
                })
                .collect();
            let preserved = mappings
                .iter()
                .filter(|mapping| matches!(mapping, ReferenceMapping::Preserved { .. }))
                .count();
            let lost = mappings.len() - preserved;
            return Ok(ReconciliationOutcome {
                to_revision: current_revision,
                preserved,
                relocated: 0,
                lost,
                mappings,
            });
        }

        // Get fresh compact observe to see current controls
        let context = self.observe_fresh().await?;
        let to_revision = context.accessibility.revision;

        // Build lookup: backend_dom_node_id -> current ref
        let mut current_by_backend: HashMap<i64, String> = HashMap::new();
        let mut current_controls: Vec<(&str, &str, i64, &str)> = Vec::new();
        let ax = &context.accessibility;
        for c in &ax.interactive {
            current_by_backend.insert(c.backend_dom_node_id, c.reference.clone());
            current_controls.push((&c.role, &c.name, c.backend_dom_node_id, &c.reference));
        }

        // A cached observation is the only safe source of the old route and
        // stable identity. If it is unavailable (for example after a process
        // restart), do not guess relocation from a raw backend ID.
        let route_changed = prior.as_ref().is_none_or(|(old_target, old_frame, _)| {
            context.page.target_id != *old_target || context.page.frame_id != *old_frame
        });

        let mut mappings = Vec::with_capacity(refs.len());
        let mut preserved = 0usize;
        let mut relocated = 0usize;
        let mut lost = 0usize;

        for old_ref in refs {
            // Parse old ref: "r<revision>:b<backend_id>"
            let Ok(Some(reference)) = parse_revisioned_reference(old_ref) else {
                mappings.push(ReferenceMapping::Lost {
                    old: old_ref.clone(),
                    reason: "invalid_ref_format".to_string(),
                });
                lost += 1;
                continue;
            };
            if reference.revision != from_revision || route_changed {
                mappings.push(ReferenceMapping::Lost {
                    old: old_ref.clone(),
                    reason: if route_changed && prior.is_some() {
                        "route_changed"
                    } else if route_changed {
                        "missing_prior_snapshot"
                    } else {
                        "stale_boundary"
                    }
                    .to_string(),
                });
                lost += 1;
                continue;
            }
            let backend_id = reference.backend_dom_node_id;

            // Try preserved (same backend node ID)
            if let Some(new_ref) = current_by_backend.get(&backend_id) {
                mappings.push(ReferenceMapping::Preserved {
                    old: old_ref.clone(),
                    new: new_ref.clone(),
                });
                preserved += 1;
                continue;
            }

            // Try an exact role+name relocation only when the old reference
            // came from the cached observation for this revision. Require one
            // candidate; duplicate labels remain Lost/Ambiguous.
            if let Some((_, _, old_controls)) = prior.as_ref()
                && let Some((_, role, name, _)) = old_controls
                    .iter()
                    .find(|(reference, _, _, _)| reference == old_ref)
            {
                let matches: Vec<_> = current_controls
                    .iter()
                    .filter(|(current_role, current_name, _, _)| {
                        current_role.eq_ignore_ascii_case(role)
                            && current_name.eq_ignore_ascii_case(name)
                    })
                    .collect();
                if matches.len() == 1 {
                    mappings.push(ReferenceMapping::Relocated {
                        old: old_ref.clone(),
                        new: matches[0].3.to_string(),
                        matched_by: "role_and_name".to_string(),
                    });
                    relocated += 1;
                    continue;
                }
                if matches.len() > 1 {
                    mappings.push(ReferenceMapping::Lost {
                        old: old_ref.clone(),
                        reason: "ambiguous".to_string(),
                    });
                    lost += 1;
                    continue;
                }
            }
            mappings.push(ReferenceMapping::Lost {
                old: old_ref.clone(),
                reason: "backend_node_removed".to_string(),
            });
            lost += 1;
        }

        Ok(ReconciliationOutcome {
            to_revision,
            mappings,
            preserved,
            relocated,
            lost,
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

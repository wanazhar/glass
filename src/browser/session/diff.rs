use super::*;
use std::collections::{HashMap, HashSet};

/// Diff between two compact accessibility observations.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AccessibilityDiff {
    pub from_revision: u64,
    pub to_revision: u64,
    pub added: Vec<DiffElement>,
    pub removed: Vec<DiffElement>,
    pub changed: Vec<DiffChange>,
    pub total_after: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffElement {
    pub reference: String,
    pub role: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffChange {
    pub reference: String,
    pub role: String,
    pub name: String,
    pub changed_properties: Vec<String>,
}

/// Compute a diff between two compact accessibility snapshots.
pub fn diff_accessibility(
    before: &CompactAccessibilitySnapshot,
    after: &CompactAccessibilitySnapshot,
) -> AccessibilityDiff {
    let before_refs: HashSet<&str> = before
        .interactive
        .iter()
        .map(|e| e.reference.as_str())
        .collect();
    let after_refs: HashSet<&str> = after
        .interactive
        .iter()
        .map(|e| e.reference.as_str())
        .collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for elem in &after.interactive {
        if !before_refs.contains(elem.reference.as_str()) {
            added.push(DiffElement {
                reference: elem.reference.clone(),
                role: elem.role.clone(),
                name: elem.name.clone(),
            });
        }
    }
    for elem in &before.interactive {
        if !after_refs.contains(elem.reference.as_str()) {
            removed.push(DiffElement {
                reference: elem.reference.clone(),
                role: elem.role.clone(),
                name: elem.name.clone(),
            });
        }
    }

    let after_by_ref: HashMap<&str, &CompactInteractiveElement> = after
        .interactive
        .iter()
        .map(|e| (e.reference.as_str(), e))
        .collect();
    for before_elem in &before.interactive {
        if let Some(after_elem) = after_by_ref.get(before_elem.reference.as_str()) {
            let mut props = Vec::new();
            if before_elem.name != after_elem.name {
                props.push("name".into());
            }
            if before_elem.role != after_elem.role {
                props.push("role".into());
            }
            if before_elem.value != after_elem.value {
                props.push("value".into());
            }
            if before_elem.checked != after_elem.checked {
                props.push("checked".into());
            }
            if before_elem.selected_option != after_elem.selected_option {
                props.push("selectedOption".into());
            }
            if before_elem.empty != after_elem.empty {
                props.push("empty".into());
            }
            if before_elem.read_only != after_elem.read_only {
                props.push("readOnly".into());
            }
            if before_elem.required != after_elem.required {
                props.push("required".into());
            }
            if !props.is_empty() {
                changed.push(DiffChange {
                    reference: before_elem.reference.clone(),
                    role: after_elem.role.clone(),
                    name: after_elem.name.clone(),
                    changed_properties: props,
                });
            }
        }
    }

    AccessibilityDiff {
        from_revision: before.revision,
        to_revision: after.revision,
        added,
        removed,
        changed,
        total_after: after.interactive.len(),
    }
}

impl BrowserSession {
    /// Diff the current observation against a prior snapshot.
    pub async fn diff_observation(
        &self,
        before: &CompactAccessibilitySnapshot,
    ) -> BrowserResult<AccessibilityDiff> {
        let ctx = self.observe().await?;
        Ok(diff_accessibility(before, &ctx.accessibility))
    }
}

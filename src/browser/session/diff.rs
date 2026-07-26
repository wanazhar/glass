//! Accessibility tree diffing.
//!
//! Computes a structured diff between two [`CompactAccessibilitySnapshot`]
//! values, identifying added, removed, and changed interactive elements.
//! Useful for verifying UI transitions after actions.

use super::*;
use std::collections::{HashMap, HashSet};

/// Diff between two compact accessibility observations.
///
/// Returned by [`diff_accessibility`] and [`BrowserSession::diff_observation`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct AccessibilityDiff {
    /// Revision of the "before" snapshot.
    pub from_revision: u64,
    /// Revision of the "after" snapshot.
    pub to_revision: u64,
    /// Elements present in "after" but not "before".
    pub added: Vec<DiffElement>,
    /// Elements present in "before" but not "after".
    pub removed: Vec<DiffElement>,
    /// Elements present in both snapshots with changed properties.
    pub changed: Vec<DiffChange>,
    /// Total interactive elements in the "after" snapshot.
    pub total_after: usize,
}

/// A single element that was added or removed in a diff.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffElement {
    /// Revisioned reference string (e.g. `"r7:b42"`).
    pub reference: String,
    /// Accessibility role (e.g. `"button"`, `"textbox"`).
    pub role: String,
    /// Accessible name of the element.
    pub name: String,
}

/// An element present in both snapshots whose properties changed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffChange {
    /// Revisioned reference string.
    pub reference: String,
    /// Current role of the element.
    pub role: String,
    /// Current accessible name.
    pub name: String,
    /// Names of the properties that changed (e.g. `["name", "value"]`).
    pub changed_properties: Vec<String>,
}

/// Compute a diff between two compact accessibility snapshots.
///
/// Elements are matched by their revisioned reference. The diff
/// detects:
///
/// - **Added**: elements only in `after`.
/// - **Removed**: elements only in `before`.
/// - **Changed**: elements in both whose `name`, `role`, `value`,
///   `checked`, `selectedOption`, `empty`, `readOnly`, or `required`
///   differ.
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
    /// Observe the current page and diff against a prior snapshot.
    ///
    /// Takes a fresh observation and computes an `AccessibilityDiff`
    /// comparing it to `before`. Useful for verifying UI state changes
    /// after performing actions.
    pub async fn diff_observation(
        &self,
        before: &CompactAccessibilitySnapshot,
    ) -> BrowserResult<AccessibilityDiff> {
        let ctx = self.observe().await?;
        Ok(diff_accessibility(before, &ctx.accessibility))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_element(reference: &str, role: &str, name: &str) -> CompactInteractiveElement {
        CompactInteractiveElement {
            reference: reference.to_string(),
            role: role.to_string(),
            name: name.to_string(),
            backend_dom_node_id: 1,
            shadow_host_path: None,
            input_type: None,
            value: None,
            checked: None,
            selected_option: None,
            empty: false,
            read_only: false,
            required: false,
        }
    }

    fn make_snapshot(
        revision: u64,
        elements: Vec<CompactInteractiveElement>,
    ) -> CompactAccessibilitySnapshot {
        CompactAccessibilitySnapshot {
            page: crate::browser::session::PageInfo {
                url: "about:blank".to_string(),
                title: String::new(),
                ready_state: "complete".to_string(),
                target_id: "t".to_string(),
                frame_id: "f".to_string(),
            },
            revision,
            roots: vec![],
            interactive: elements,
            truncated: false,
            omitted_count: 0,
            ranking_applied: false,
            completeness: None,
        }
    }

    #[test]
    fn diff_same_elements_produces_no_changes() {
        let elements = vec![make_element("r1:b42", "button", "Save")];
        let before = make_snapshot(1, elements.clone());
        let after = make_snapshot(2, elements);
        let diff = diff_accessibility(&before, &after);
        assert_eq!(diff.from_revision, 1);
        assert_eq!(diff.to_revision, 2);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.changed.is_empty());
        assert_eq!(diff.total_after, 1);
    }

    #[test]
    fn diff_detects_name_change() {
        let before = make_snapshot(1, vec![make_element("r1:b1", "button", "Save")]);
        let after = make_snapshot(2, vec![make_element("r1:b1", "button", "Submit")]);
        let diff = diff_accessibility(&before, &after);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].reference, "r1:b1");
        assert!(
            diff.changed[0]
                .changed_properties
                .contains(&"name".to_string())
        );
    }

    #[test]
    fn diff_detects_role_change() {
        let before = make_snapshot(1, vec![make_element("r1:b1", "button", "OK")]);
        let after = make_snapshot(2, vec![make_element("r1:b1", "link", "OK")]);
        let diff = diff_accessibility(&before, &after);
        assert_eq!(diff.changed.len(), 1);
        assert!(
            diff.changed[0]
                .changed_properties
                .contains(&"role".to_string())
        );
    }

    #[test]
    fn diff_detects_added_and_removed_elements() {
        let before = make_snapshot(1, vec![make_element("r1:b1", "button", "Old")]);
        let after = make_snapshot(2, vec![make_element("r2:b2", "button", "New")]);
        let diff = diff_accessibility(&before, &after);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].reference, "r2:b2");
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].reference, "r1:b1");
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn diff_detects_value_change() {
        let mut before_elem = make_element("r1:b1", "textbox", "Search");
        before_elem.value = Some("old".to_string());
        let mut after_elem = make_element("r1:b1", "textbox", "Search");
        after_elem.value = Some("new".to_string());
        let before = make_snapshot(1, vec![before_elem]);
        let after = make_snapshot(2, vec![after_elem]);
        let diff = diff_accessibility(&before, &after);
        assert_eq!(diff.changed.len(), 1);
        assert!(
            diff.changed[0]
                .changed_properties
                .contains(&"value".to_string())
        );
    }

    #[test]
    fn diff_detects_checked_state_change() {
        let mut before_elem = make_element("r1:b1", "checkbox", "Agree");
        before_elem.checked = Some(false);
        let mut after_elem = make_element("r1:b1", "checkbox", "Agree");
        after_elem.checked = Some(true);
        let before = make_snapshot(1, vec![before_elem]);
        let after = make_snapshot(2, vec![after_elem]);
        let diff = diff_accessibility(&before, &after);
        assert_eq!(diff.changed.len(), 1);
        assert!(
            diff.changed[0]
                .changed_properties
                .contains(&"checked".to_string())
        );
    }
}

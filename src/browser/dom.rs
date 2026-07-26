//! DOM and accessibility tree parsing.
//!
//! Parses CDP `DOM.getDocument` and `Accessibility.getFullAXTree` responses
//! into typed Rust structures. Provides compact accessibility projection
//! with configurable node and text limits.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Hard limits for accessibility data retained by a compact observation.
pub const COMPACT_AX_MAX_NODES: usize = 128;
pub const COMPACT_AX_MAX_INTERACTIVE: usize = 32;
pub const COMPACT_AX_TEXT_MAX_BYTES: usize = 4 * 1024;
pub const COMPACT_AX_ROLE_MAX_BYTES: usize = 64;
const COMPACT_AX_INTERACTIVE_TEXT_MAX_BYTES: usize = COMPACT_AX_TEXT_MAX_BYTES / 2;
const COMPACT_AX_OUTLINE_TEXT_MAX_BYTES: usize =
    COMPACT_AX_TEXT_MAX_BYTES - COMPACT_AX_INTERACTIVE_TEXT_MAX_BYTES;
const COMPACT_AX_TRUNCATION_MARKER: &str = "…";

/// Shadow piercing limits for compact observation.
pub const MAX_SHADOW_DEPTH: u8 = 3;
pub const MAX_SHADOW_HOSTS: usize = 8;
pub const MAX_SHADOW_PATH_ENTRIES: usize = 3;
pub const MAX_SHADOW_PATH_ENTRY_BYTES: usize = 64;

/// A simplified accessibility tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxNode {
    pub ax_node_id: String,
    pub backend_dom_node_id: Option<i64>,
    pub role: String,
    pub name: String,
    pub description: String,
    pub value: Option<String>,
    pub children: Vec<AxNode>,
    /// Bounding box: [x, y, width, height]. Filled by the session when needed.
    pub bounds: Option<[f64; 4]>,
    pub interactive: bool,
    /// HTML input type extracted from AX properties (text, email, checkbox, etc.).
    pub input_type: Option<String>,
}

/// A bounded semantic accessibility node used only by compact observations.
#[derive(Debug, Clone, Serialize)]
pub struct CompactAxNode {
    pub role: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<CompactAxNode>,
    #[serde(skip_serializing_if = "is_false")]
    pub interactive: bool,
}

/// A bounded interactive control included in a compact observation.
#[derive(Debug, Clone, Serialize)]
pub struct CompactInteractiveElement {
    pub reference: String,
    pub role: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub backend_dom_node_id: i64,
    /// Bounded accessibility ancestor labels used for scoped reconciliation.
    /// This is an internal continuity hint and is not emitted on the wire.
    #[serde(skip)]
    pub(crate) ancestor_path: Vec<String>,
    /// Role+name breadcrumbs for shadow-host ancestry (max 3 entries, 64 bytes each).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_host_path: Option<Vec<String>>,
    /// HTML input type for form controls (text, email, password, checkbox, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    // --- Form value fields (populated only when includeFormValues is set) ---
    /// Current value of the form control, bounded to 256 bytes. Redacted for passwords.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Checked state for checkbox/radio controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    /// Selected option label for `<select>` elements, bounded to 128 bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_option: Option<String>,
    /// Whether the field is empty (no user input).
    #[serde(skip_serializing_if = "is_false")]
    pub empty: bool,
    /// Whether the field is read-only.
    #[serde(skip_serializing_if = "is_false")]
    pub read_only: bool,
    /// Whether the field has the required attribute.
    #[serde(skip_serializing_if = "is_false")]
    pub required: bool,
}

/// Maximum UTF-8 byte length for a form field value in compact observe.
pub const FORM_VALUE_MAX_BYTES: usize = 256;
/// Maximum UTF-8 byte length for a `<select>` option label.
pub const SELECT_OPTION_MAX_BYTES: usize = 128;
/// Maximum number of form fields whose values are read per observe.
pub const FORM_VALUE_MAX_FIELDS: usize = 16;

/// Compact accessibility data with no unbounded text fields.
#[derive(Debug, Clone)]
pub struct CompactAccessibilityProjection {
    pub roots: Vec<CompactAxNode>,
    pub interactive: Vec<CompactInteractiveElement>,
    pub truncated: bool,
    pub nodes_truncated: bool,
    pub labels_truncated: bool,
    pub controls_truncated: bool,
    /// Total interactive controls discovered (before truncation/ranking).
    pub interactive_discovered: usize,
    /// Number of interactive controls discovered but omitted due to the 32-control budget.
    pub omitted_count: usize,
    /// Whether the interactive list was relevance-ranked before truncation.
    pub ranking_applied: bool,
    /// Shadow piercing summary.
    pub shadow_pierced: ShadowPiercedSummary,
}

/// Summary of shadow-root piercing during a compact observation.
#[derive(Debug, Clone, Default)]
pub struct ShadowPiercedSummary {
    /// Number of open shadow hosts visited during piercing.
    pub hosts_visited: usize,
    /// Number of interactive controls discovered inside shadow roots.
    pub controls_found: usize,
    /// Whether piercing was truncated by budget (depth > 3 or hosts > 8).
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactRanking {
    Relevance,
    DocumentOrder,
}

struct CompactProjectionState {
    node_count: usize,
    interactive_text_remaining: usize,
    outline_text_remaining: usize,
    interactive: Vec<CompactInteractiveElement>,
    truncated: bool,
    nodes_truncated: bool,
    labels_truncated: bool,
    controls_truncated: bool,
}

impl CompactProjectionState {
    fn new() -> Self {
        Self {
            node_count: 0,
            interactive_text_remaining: COMPACT_AX_INTERACTIVE_TEXT_MAX_BYTES,
            outline_text_remaining: COMPACT_AX_OUTLINE_TEXT_MAX_BYTES,
            interactive: Vec::new(),
            truncated: false,
            nodes_truncated: false,
            labels_truncated: false,
            controls_truncated: false,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Extract a linked accessibility tree from `Accessibility.getFullAXTree`.
pub fn parse_accessibility_tree(raw: &Value) -> Vec<AxNode> {
    let Some(entries) = raw["nodes"].as_array() else {
        return Vec::new();
    };

    let mut by_id = HashMap::new();
    for entry in entries {
        if let Some(id) = entry["nodeId"].as_str() {
            by_id.insert(id.to_string(), entry.clone());
        }
    }

    let mut child_ids = HashSet::new();
    for entry in entries {
        if let Some(children) = entry["childIds"].as_array() {
            for child in children.iter().filter_map(Value::as_str) {
                child_ids.insert(child.to_string());
            }
        }
    }

    let mut roots = Vec::new();
    for entry in entries {
        let Some(id) = entry["nodeId"].as_str() else {
            continue;
        };
        let has_parent = entry["parentId"].as_str().is_some() || child_ids.contains(id);
        if !has_parent {
            roots.push(id.to_string());
        }
    }
    if roots.is_empty() {
        roots.extend(
            entries
                .iter()
                .filter_map(|entry| entry["nodeId"].as_str().map(String::from)),
        );
    }

    let mut visiting = HashSet::new();
    roots
        .iter()
        .filter_map(|id| build_ax_node(id, &by_id, &mut visiting))
        .collect()
}

fn build_ax_node(
    id: &str,
    by_id: &HashMap<String, Value>,
    visiting: &mut HashSet<String>,
) -> Option<AxNode> {
    if !visiting.insert(id.to_string()) {
        return None;
    }
    let raw = by_id.get(id)?;
    let children = raw["childIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|child_id| build_ax_node(child_id, by_id, visiting))
        .collect();
    visiting.remove(id);

    let role = property_text(raw, "role").unwrap_or_else(|| "unknown".to_string());
    let interactive = is_interactive_role(&role)
        || raw["properties"].as_array().is_some_and(|properties| {
            properties.iter().any(|property| {
                property["name"].as_str() == Some("focusable")
                    && property["value"]["value"].as_bool() == Some(true)
            })
        });

    Some(AxNode {
        ax_node_id: id.to_string(),
        backend_dom_node_id: raw["backendDOMNodeId"].as_i64(),
        role,
        name: property_text(raw, "name").unwrap_or_default(),
        description: property_text(raw, "description").unwrap_or_default(),
        value: property_text(raw, "value"),
        children,
        bounds: None,
        interactive,
        input_type: extract_input_type(raw),
    })
}

fn property_text(raw: &Value, property: &str) -> Option<String> {
    raw[property]["value"]
        .as_str()
        .or_else(|| raw[property].as_str())
        .map(String::from)
}

fn is_interactive_role(role: &str) -> bool {
    matches!(
        role,
        "button"
            | "link"
            | "textbox"
            | "checkbox"
            | "radio"
            | "combobox"
            | "menuitem"
            | "tab"
            | "option"
            | "searchbox"
            | "spinbutton"
            | "slider"
            | "switch"
    )
}

/// Return accessibility nodes with the `interactive` flag set, in document
/// order (depth-first).
pub fn find_interactive_elements(nodes: &[AxNode]) -> Vec<&AxNode> {
    let mut result = Vec::new();
    for node in nodes {
        if node.interactive {
            result.push(node);
        }
        result.extend(find_interactive_elements(&node.children));
    }
    result
}

/// Deterministic relevance score for interactive controls.
/// Higher score = more important to include when truncating.
fn control_relevance_score(control: &CompactInteractiveElement) -> i32 {
    let role_score = match control.role.as_str() {
        "button" | "link" => 36,
        "textbox" | "combobox" | "searchbox" | "spinbutton" => 34,
        "checkbox" | "radio" | "switch" | "menuitem" | "option" => 30,
        "slider" => 28,
        "listbox" | "menu" | "tab" | "treeitem" => 24,
        _ => 12,
    };
    let form_score = if control.input_type.is_some() { 8 } else { 0 }
        + i32::from(control.required) * 5
        + i32::from(control.checked.is_some() || control.selected_option.is_some()) * 3;
    let name_score = if control.name.is_empty() { 0 } else { 6 };
    let shadow_score = if control.shadow_host_path.is_some() {
        -2
    } else {
        0
    };
    role_score + form_score + name_score + shadow_score
}

/// Rank interactive controls by relevance, keeping the most important ones first.
/// Stable sort preserves document order for equal scores (tie-break).
fn rank_interactive_controls(controls: &mut [CompactInteractiveElement]) {
    controls.sort_by(|a, b| {
        let score_a = control_relevance_score(a);
        let score_b = control_relevance_score(b);
        score_b.cmp(&score_a)
    });
}

/// Project a full accessibility tree into bounded semantic context.
///
/// Every published control has a backend DOM node and a revisioned reference,
/// so it can be resolved without a fresh full accessibility snapshot.
pub fn project_compact_accessibility(
    nodes: &[AxNode],
    revision: u64,
) -> CompactAccessibilityProjection {
    project_compact_accessibility_with_ranking(nodes, revision, CompactRanking::Relevance)
}

/// Project compact accessibility with an explicit truncation order. The
/// document-order mode is a compatibility escape hatch for regression tests
/// and callers comparing pre-ranking observations.
pub fn project_compact_accessibility_with_ranking(
    nodes: &[AxNode],
    revision: u64,
    ranking: CompactRanking,
) -> CompactAccessibilityProjection {
    let mut state = CompactProjectionState::new();
    let roots = nodes
        .iter()
        .filter_map(|node| project_compact_node(node, revision, &mut state, &[]))
        .collect();
    let total_discovered = state.interactive.len();
    let ranking_applied =
        total_discovered > COMPACT_AX_MAX_INTERACTIVE && ranking == CompactRanking::Relevance;

    if total_discovered > COMPACT_AX_MAX_INTERACTIVE {
        if ranking == CompactRanking::Relevance {
            rank_interactive_controls(&mut state.interactive);
        }
        let omitted = total_discovered.saturating_sub(COMPACT_AX_MAX_INTERACTIVE);
        state.interactive.truncate(COMPACT_AX_MAX_INTERACTIVE);
        state.controls_truncated = true;

        CompactAccessibilityProjection {
            roots,
            interactive: state.interactive,
            truncated: state.truncated,
            nodes_truncated: state.nodes_truncated,
            labels_truncated: state.labels_truncated,
            controls_truncated: true,
            interactive_discovered: total_discovered,
            omitted_count: omitted.min(999),
            ranking_applied,
            shadow_pierced: ShadowPiercedSummary::default(),
        }
    } else {
        CompactAccessibilityProjection {
            roots,
            interactive: state.interactive,
            truncated: state.truncated,
            nodes_truncated: state.nodes_truncated,
            labels_truncated: state.labels_truncated,
            controls_truncated: state.controls_truncated,
            interactive_discovered: total_discovered,
            omitted_count: 0,
            ranking_applied: false,
            shadow_pierced: ShadowPiercedSummary::default(),
        }
    }
}

fn project_compact_node(
    node: &AxNode,
    revision: u64,
    state: &mut CompactProjectionState,
    ancestors: &[String],
) -> Option<CompactAxNode> {
    if state.node_count >= COMPACT_AX_MAX_NODES {
        state.truncated = true;
        state.nodes_truncated = true;
        return None;
    }
    state.node_count += 1;

    let (role, role_truncated) = truncate_utf8(&node.role, COMPACT_AX_ROLE_MAX_BYTES);
    state.truncated |= role_truncated;
    state.labels_truncated |= role_truncated;
    let mut name = String::new();
    if node.interactive {
        let control_name = take_compact_text(
            &node.name,
            &mut state.interactive_text_remaining,
            &mut state.truncated,
        );
        state.controls_truncated |= control_name.len() < node.name.len();
        if let Some(backend_dom_node_id) = node.backend_dom_node_id {
            state.interactive.push(CompactInteractiveElement {
                reference: backend_node_reference(revision, backend_dom_node_id),
                role: role.clone(),
                name: control_name,
                backend_dom_node_id,
                ancestor_path: ancestors.to_vec(),
                shadow_host_path: None,
                input_type: node.input_type.clone(),
                value: None,
                checked: None,
                selected_option: None,
                empty: false,
                read_only: false,
                required: false,
            });
        } else {
            state.truncated = true;
            state.controls_truncated = true;
        }
    } else {
        name = take_compact_text(
            &node.name,
            &mut state.outline_text_remaining,
            &mut state.truncated,
        );
        state.labels_truncated |= name.len() < node.name.len();
    }

    let mut child_ancestors = ancestors.to_vec();
    let ancestor_label = format!("{}:{}", role, name);
    if !ancestor_label.trim_end_matches(':').is_empty() {
        let (label, _) = truncate_utf8(&ancestor_label, 96);
        child_ancestors.push(label);
        if child_ancestors.len() > 4 {
            child_ancestors.remove(0);
        }
    }
    let children = node
        .children
        .iter()
        .filter_map(|child| project_compact_node(child, revision, state, &child_ancestors))
        .collect();
    Some(CompactAxNode {
        role,
        name,
        children,
        interactive: node.interactive,
    })
}

/// Format a stable-in-one-revision element reference backed by Chrome's DOM
/// node identifier. The revision makes stale references fail rather than
/// silently selecting an element that inherited an ordinal position.
pub fn backend_node_reference(revision: u64, backend_dom_node_id: i64) -> String {
    format!("r{revision}:b{backend_dom_node_id}")
}

fn take_compact_text(text: &str, remaining: &mut usize, truncated: &mut bool) -> String {
    if text.is_empty() {
        return String::new();
    }
    let (value, was_truncated) = truncate_utf8(text, *remaining);
    *remaining = remaining.saturating_sub(value.len());
    *truncated |= was_truncated;
    value
}

pub(crate) fn truncate_utf8(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }

    let marker_len = COMPACT_AX_TRUNCATION_MARKER.len();
    let mut end = max_bytes.saturating_sub(marker_len);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut value = text[..end].to_string();
    if max_bytes >= marker_len {
        value.push_str(COMPACT_AX_TRUNCATION_MARKER);
    }
    (value, true)
}

/// Recursively collect visible name text from an accessibility subtree,
/// joined with newlines.
pub fn extract_text_content(nodes: &[AxNode]) -> String {
    let mut parts = Vec::new();
    extract_text_recursive(nodes, &mut parts);
    parts.join("\n")
}

fn extract_text_recursive(nodes: &[AxNode], parts: &mut Vec<String>) {
    for node in nodes {
        if !node.name.is_empty() && node.role != "presentation" && node.role != "none" {
            parts.push(node.name.clone());
        }
        extract_text_recursive(&node.children, parts);
    }
}

/// Format an accessibility tree as a human-readable indented string for
/// debugging and inspection.
pub fn format_tree(nodes: &[AxNode], indent: usize) -> String {
    let mut output = String::new();
    for node in nodes {
        let prefix = "  ".repeat(indent);
        let interactive_marker = if node.interactive {
            " [interactive]"
        } else {
            ""
        };
        let value_preview = node
            .value
            .as_ref()
            .map(|value| {
                let preview: String = value.chars().take(50).collect();
                format!(" = \"{preview}\"")
            })
            .unwrap_or_default();

        output.push_str(&format!(
            "{}[{}] \"{}\"{}{}\n",
            prefix, node.role, node.name, value_preview, interactive_marker
        ));
        output.push_str(&format_tree(&node.children, indent + 1));
    }
    output
}

/// Build a mapping from backend DOM node IDs to their shadow host path breadcrumbs
/// by walking a flattened DOM document response.
pub fn build_shadow_host_paths(flattened_doc: &Value) -> HashMap<i64, Vec<String>> {
    let mut paths: HashMap<i64, Vec<String>> = HashMap::new();
    let Some(nodes) = flattened_doc["nodes"].as_array() else {
        return paths;
    };

    // Build lookup tables: nodeId -> (backendNodeId, parentNodeId, isShadowRoot, label)
    struct NodeMeta {
        parent_id: Option<i64>,
        is_shadow_root: bool,
        label: String,
    }
    let mut node_meta: HashMap<i64, NodeMeta> = HashMap::new();

    for node in nodes {
        let Some(node_id) = node["nodeId"].as_i64() else {
            continue;
        };
        // Skip nodes without a backendNodeId — they can't be interactive controls
        if node["backendNodeId"].as_i64().is_none() {
            continue;
        };

        let is_shadow_root = node["shadowRootType"].as_str() == Some("open");
        let parent_id = node["parentId"].as_i64();

        // Build a human-readable label for the node
        let tag = node["localName"]
            .as_str()
            .or_else(|| node["nodeName"].as_str())
            .filter(|n| !n.starts_with('#'))
            .unwrap_or("");
        let role_attr = node["attributes"]
            .as_array()
            .iter()
            .flat_map(|a| a.iter())
            .collect::<Vec<_>>()
            .windows(2)
            .find(|w| w[0].as_str() == Some("role"))
            .and_then(|w| w[1].as_str());
        let label = if let Some(role) = role_attr {
            truncate_utf8(role, MAX_SHADOW_PATH_ENTRY_BYTES).0
        } else if !tag.is_empty() {
            truncate_utf8(tag, MAX_SHADOW_PATH_ENTRY_BYTES).0
        } else {
            "shadow-host".to_string()
        };

        node_meta.insert(
            node_id,
            NodeMeta {
                parent_id,
                is_shadow_root,
                label,
            },
        );
    }

    if node_meta.is_empty() {
        return paths;
    }

    // For each leaf node (potential interactive control), trace up to find shadow boundaries
    for node in nodes {
        let Some(backend_id) = node["backendNodeId"].as_i64() else {
            continue;
        };
        let Some(mut current_parent) = node["parentId"].as_i64() else {
            continue;
        };

        let mut breadcrumbs: Vec<String> = Vec::new();
        let mut visited = HashSet::new();
        let mut hosts_found = 0usize;

        loop {
            if !visited.insert(current_parent) || breadcrumbs.len() >= MAX_SHADOW_PATH_ENTRIES {
                break;
            }

            let Some(meta) = node_meta.get(&current_parent) else {
                // Parent not in our metadata — walk up further
                if let Some(parent_of_parent) =
                    node_meta.get(&current_parent).and_then(|m| m.parent_id)
                {
                    current_parent = parent_of_parent;
                    continue;
                }
                break;
            };

            if meta.is_shadow_root {
                hosts_found += 1;
                breadcrumbs.push(meta.label.clone());
                // Move past the shadow root to its host's parent
                if let Some(next) = meta.parent_id {
                    current_parent = next;
                } else {
                    break;
                }
            } else if hosts_found > 0 {
                // Once we've found at least one shadow boundary, continue up for nested shadows
                if let Some(next) = meta.parent_id {
                    current_parent = next;
                } else {
                    break;
                }
            } else {
                break; // Not inside a shadow tree
            }
        }

        if !breadcrumbs.is_empty() {
            breadcrumbs.reverse();
            paths.insert(backend_id, breadcrumbs);
        }
    }

    paths
}

/// Count unique shadow hosts that have pierced controls in the interactive list.
/// Each host's backendNodeId in the shadow_host_paths map represents one pierced host.
pub fn count_pierced_shadow_hosts(shadow_paths: &HashMap<i64, Vec<String>>) -> usize {
    // Each path entry represents nesting; count unique first breadcrumb as host
    let mut hosts = HashSet::new();
    for path in shadow_paths.values() {
        if let Some(first) = path.last() {
            hosts.insert(first.clone());
        }
    }
    hosts.len().min(MAX_SHADOW_HOSTS)
}

/// Extract the HTML input type from AX node properties if available.
pub fn extract_input_type(raw_ax_node: &Value) -> Option<String> {
    raw_ax_node["properties"]
        .as_array()?
        .iter()
        .find(|prop| prop["name"].as_str() == Some("inputType"))
        .and_then(|prop| prop["value"]["value"].as_str())
        .map(String::from)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomNode {
    pub node_id: i64,
    pub node_name: String,
    pub node_value: String,
    pub children: Vec<DomNode>,
    pub attributes: Vec<String>,
    pub bounding_box: Option<[f64; 4]>,
}

/// Parse a CDP `DOM.getDocument` response into a [`DomNode`] tree. Returns
/// `None` when the `root` field is absent or malformed.
pub fn parse_dom_tree(raw: &Value) -> Option<DomNode> {
    raw.get("root").and_then(parse_dom_node)
}

fn parse_dom_node(raw: &Value) -> Option<DomNode> {
    let node_id = raw["nodeId"].as_i64()?;
    let node_name = raw["nodeName"].as_str().unwrap_or_default().to_string();
    let node_value = raw["nodeValue"].as_str().unwrap_or_default().to_string();
    let attributes = raw["attributes"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let children = raw["children"]
        .as_array()
        .map(|values| values.iter().filter_map(parse_dom_node).collect())
        .unwrap_or_default();

    Some(DomNode {
        node_id,
        node_name,
        node_value,
        children,
        attributes,
        bounding_box: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_accessibility_children_and_interactive_index() {
        let raw = serde_json::json!({
            "nodes": [
                {"nodeId": "root", "role": {"value": "RootWebArea"}, "childIds": ["button"]},
                {"nodeId": "button", "parentId": "root", "backendDOMNodeId": 42,
                 "role": {"value": "button"}, "name": {"value": "Save"}, "childIds": []}
            ]
        });
        let tree = parse_accessibility_tree(&raw);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children[0].name, "Save");
        assert_eq!(
            find_interactive_elements(&tree)[0].backend_dom_node_id,
            Some(42)
        );
        assert!(format_tree(&tree, 0).contains("[interactive]"));
    }

    #[test]
    fn formats_unicode_values_without_slicing_bytes() {
        let node = AxNode {
            ax_node_id: "1".to_string(),
            backend_dom_node_id: None,
            role: "textbox".to_string(),
            name: "入力".to_string(),
            description: String::new(),
            value: Some("日本語".to_string()),
            children: Vec::new(),
            bounds: None,
            interactive: true,
            input_type: Some("text".to_string()),
        };
        assert!(format_tree(&[node], 0).contains("日本語"));
    }

    #[test]
    fn compact_projection_publishes_revisioned_backend_references_only() {
        let nodes = vec![AxNode {
            ax_node_id: "root".to_string(),
            backend_dom_node_id: None,
            role: "RootWebArea".to_string(),
            name: String::new(),
            description: String::new(),
            value: None,
            children: vec![
                AxNode {
                    ax_node_id: "save".to_string(),
                    backend_dom_node_id: Some(42),
                    role: "button".to_string(),
                    name: "Save".to_string(),
                    description: String::new(),
                    value: None,
                    children: Vec::new(),
                    bounds: None,
                    interactive: true,
                    input_type: None,
                },
                AxNode {
                    ax_node_id: "unresolved".to_string(),
                    backend_dom_node_id: None,
                    role: "button".to_string(),
                    name: "Unresolved".to_string(),
                    description: String::new(),
                    value: None,
                    children: Vec::new(),
                    bounds: None,
                    interactive: true,
                    input_type: None,
                },
            ],
            bounds: None,
            interactive: false,
            input_type: None,
        }];

        let projection = project_compact_accessibility(&nodes, 5);
        assert_eq!(projection.interactive.len(), 1);
        assert_eq!(projection.interactive[0].reference, "r5:b42");
        assert_eq!(projection.interactive[0].backend_dom_node_id, 42);
        assert!(projection.truncated);
    }
}

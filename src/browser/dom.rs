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
}

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
    match control.role.as_str() {
        "button" | "link" => 20,
        "textbox" | "combobox" | "searchbox" | "spinbutton" => 18,
        "checkbox" | "radio" | "switch" | "menuitem" | "option" => 15,
        "listbox" | "menu" | "tab" | "treeitem" => 10,
        _ => 0,
    }
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
    let mut state = CompactProjectionState::new();
    let roots = nodes
        .iter()
        .filter_map(|node| project_compact_node(node, revision, &mut state))
        .collect();
    let total_discovered = state.interactive.len();
    let ranking_applied = total_discovered > COMPACT_AX_MAX_INTERACTIVE;

    if ranking_applied {
        rank_interactive_controls(&mut state.interactive);
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
            ranking_applied: true,
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
        }
    }
}

fn project_compact_node(
    node: &AxNode,
    revision: u64,
    state: &mut CompactProjectionState,
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

    let children = node
        .children
        .iter()
        .filter_map(|child| project_compact_node(child, revision, state))
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

fn truncate_utf8(text: &str, max_bytes: usize) -> (String, bool) {
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

/// Parse a `DOM.getDocument` response into a simplified node structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomNode {
    pub node_id: i64,
    pub node_name: String,
    pub node_value: String,
    pub children: Vec<DomNode>,
    pub attributes: Vec<String>,
    pub bounding_box: Option<[f64; 4]>,
}

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
                },
            ],
            bounds: None,
            interactive: false,
        }];

        let projection = project_compact_accessibility(&nodes, 5);
        assert_eq!(projection.interactive.len(), 1);
        assert_eq!(projection.interactive[0].reference, "r5:b42");
        assert_eq!(projection.interactive[0].backend_dom_node_id, 42);
        assert!(projection.truncated);
    }
}

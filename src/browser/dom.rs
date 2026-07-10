use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
        roots.extend(by_id.keys().cloned());
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
}

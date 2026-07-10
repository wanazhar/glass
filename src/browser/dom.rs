use serde::{Deserialize, Serialize};

/// A simplified accessibility tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxNode {
    pub backend_dom_node_id: Option<i64>,
    pub role: String,
    pub name: String,
    pub description: String,
    pub value: Option<String>,
    pub children: Vec<AxNode>,
    /// Bounding box: [x, y, width, height]
    pub bounds: Option<[f64; 4]>,
    /// Whether the node is clickable/interactive
    pub interactive: bool,
}

/// Extract the accessibility tree from CDP response and build a simplified tree.
pub fn parse_accessibility_tree(raw: &serde_json::Value) -> Vec<AxNode> {
    let mut nodes = Vec::new();

    if let Some(tree) = raw["nodes"].as_array() {
        // Build a map of backend_node_id -> node info
        let mut node_map: std::collections::HashMap<i64, serde_json::Value> =
            std::collections::HashMap::new();

        for entry in tree {
            if let Some(id) = entry["backendDOMNodeId"].as_i64() {
                node_map.insert(id, entry.clone());
            }
        }

        // Find root nodes (no parent)
        for entry in tree {
            if entry["parentAXNodeId"].is_null() || entry["parentAXNodeId"].as_str().is_none() {
                if let Some(ax_node) = build_ax_node(entry, &node_map) {
                    nodes.push(ax_node);
                }
            }
        }
    }

    nodes
}

fn build_ax_node(
    raw: &serde_json::Value,
    _node_map: &std::collections::HashMap<i64, serde_json::Value>,
) -> Option<AxNode> {
    let role = raw["role"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let name = raw["name"]["value"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let description = raw["description"]["value"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let value = raw["value"]["value"]
        .as_str()
        .map(String::from);

    let backend_id = raw["backendDOMNodeId"].as_i64();

    let interactive = matches!(
        role.as_str(),
        "button" | "link" | "textbox" | "checkbox" | "radio"
            | "combobox" | "menuitem" | "tab" | "option"
            | "searchbox" | "spinbutton" | "slider" | "switch"
    );

    let children = Vec::new(); // Simplified: don't recurse for now

    Some(AxNode {
        backend_dom_node_id: backend_id,
        role,
        name,
        description,
        value,
        children,
        bounds: None,
        interactive,
    })
}

/// Extract all interactive elements from the accessibility tree.
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

/// Get page text content by extracting all text from the accessibility tree.
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

/// Format the accessibility tree as a human-readable string.
pub fn format_tree(nodes: &[AxNode], indent: usize) -> String {
    let mut output = String::new();
    for node in nodes {
        let prefix = "  ".repeat(indent);
        let interactive_marker = if node.interactive { " [interactive]" } else { "" };
        let value_preview = node
            .value
            .as_ref()
            .map(|v| format!(" = \"{}\"", &v[..v.len().min(50)]))
            .unwrap_or_default();

        output.push_str(&format!(
            "{}[{}] \"{}\"{}{}\n",
            prefix, node.role, node.name, value_preview, interactive_marker
        ));

        output.push_str(&format_tree(&node.children, indent + 1));
    }
    output
}

/// Parse DOM document into a simplified node structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomNode {
    pub node_id: i64,
    pub node_name: String,
    pub node_value: String,
    pub children: Vec<DomNode>,
    pub attributes: Vec<String>,
    pub bounding_box: Option<[f64; 4]>,
}

/// Parse the DOM.getDocument response into our DomNode tree.
pub fn parse_dom_tree(raw: &serde_json::Value) -> Option<DomNode> {
    raw.get("root").and_then(|root| parse_dom_node(root))
}

fn parse_dom_node(raw: &serde_json::Value) -> Option<DomNode> {
    let node_id = raw["nodeId"].as_i64()?;
    let node_name = raw["nodeName"].as_str().unwrap_or("").to_string();
    let node_value = raw["nodeValue"].as_str().unwrap_or("").to_string();

    let attributes = raw["attributes"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let children = raw["children"]
        .as_array()
        .map(|arr| arr.iter().filter_map(parse_dom_node).collect())
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

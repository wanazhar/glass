//! Small headless web UI inspection contract for the glass-marble web surface.
//!
//! The first forward-fit slice of the headless UI inspection path dumps a
//! stable element tree and captures one screenshot for a local web UI page.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable element identity plus semantic content for one inspected UI node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiElement {
    /// Stable element identity shared by inspection, actions, and tests.
    pub node_id: String,
    /// Stable accessibility role of the inspected element.
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl WebUiElement {
    /// Stable element identity for one inspected node.
    pub fn stable_node_id(index: usize) -> String {
        format!("web-ui-element:{index}")
    }
}

/// Headless inspection outcome for one local web UI page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiInspection {
    /// Stable element tree dumped for the inspected page.
    pub elements: Vec<WebUiElement>,
    /// Captured screenshot payload for the inspected page.
    pub screenshot: String,
}

impl WebUiInspection {
    /// Dump the inspected page as one stable element tree payload.
    pub fn element_tree(&self) -> Value {
        serde_json::to_value(&self.elements).unwrap_or(Value::Null)
    }

    /// Captured screenshot payload for the inspected page.
    pub fn screenshot(&self) -> &str {
        &self.screenshot
    }

    /// Number of elements in the stable tree.
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_ui_inspection_dumps_stable_element_tree_and_captured_screenshot() {
        let inspection = WebUiInspection {
            elements: vec![WebUiElement {
                node_id: WebUiElement::stable_node_id(0),
                role: "button".to_string(),
                name: Some("Send".to_string()),
                text: None,
            }],
            screenshot: "screenshot-png".to_string(),
        };

        assert_eq!(inspection.element_count(), 1);
        assert_eq!(inspection.screenshot(), "screenshot-png");

        let tree = inspection.element_tree();
        let expected = serde_json::to_value(&vec![WebUiElement {
            node_id: WebUiElement::stable_node_id(0),
            role: "button".to_string(),
            name: Some("Send".to_string()),
            text: None,
        }])
        .unwrap_or(Value::Null);
        assert_eq!(tree, expected);
    }
}

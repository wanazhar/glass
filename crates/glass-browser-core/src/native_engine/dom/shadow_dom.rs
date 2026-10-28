use super::{NativeDocument, NativeNodeId, NativeNodeKind};

impl NativeDocument {
    /// Return the children used by the native composed-tree projections.
    /// DOM queries continue to use the raw child lists and do not cross a
    /// shadow boundary.
    pub(crate) fn composed_children(&self, parent: NativeNodeId) -> Vec<NativeNodeId> {
        let Some(node) = self.node(parent) else {
            return Vec::new();
        };

        if let Some(root) = node.shadow_root {
            return self
                .node(root)
                .map(|root| root.children().to_vec())
                .unwrap_or_default();
        }

        if node.element_name() == Some("slot")
            && let Some(host) = self.shadow_host_for_node(parent)
        {
            let slot_name = node.attribute("name").unwrap_or_default();
            let assigned = self
                .node(host)
                .map(|host| {
                    host.children()
                        .iter()
                        .copied()
                        .filter(|child| {
                            let Some(child) = self.node(*child) else {
                                return false;
                            };
                            child.element_name().is_some()
                                && child.attribute("slot").unwrap_or_default() == slot_name
                                || matches!(child.kind(), NativeNodeKind::Text(_))
                                    && slot_name.is_empty()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !assigned.is_empty() {
                return assigned;
            }
        }

        node.children().to_vec()
    }

    fn shadow_host_for_node(&self, mut id: NativeNodeId) -> Option<NativeNodeId> {
        for _ in 0..=self.max_dom_depth {
            let node = self.node(id)?;
            if let Some(host) = node.shadow_host {
                return Some(host);
            }
            id = node.parent()?;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_engine::{
        config::{NativeEngineLimits, Viewport},
        javascript::{NativeJavaScriptRuntime, NativeScriptCommand},
        origin::NativeOrigin,
    };

    #[test]
    fn shadow_children_replace_host_light_children_in_visible_projection() {
        let mut document = NativeDocument::parse(
            "<!doctype html><html><body><x-host>LIGHT_TEXT</x-host></body></html>",
            &NativeEngineLimits::default(),
        )
        .expect("document parses");
        let host = document
            .find_element(document.root(), "x-host")
            .expect("host exists");
        let root_index = u32::MAX - 3;
        let child_index = u32::MAX - 2;
        let text_index = u32::MAX - 1;
        document
            .apply_script_commands(&[
                NativeScriptCommand::CreateShadowRoot {
                    host_index: host.index(),
                    node_index: root_index,
                    mode: "open".into(),
                },
                NativeScriptCommand::CreateElement {
                    node_index: child_index,
                    tag_name: "span".into(),
                    namespace_uri: None,
                },
                NativeScriptCommand::CreateTextNode {
                    node_index: text_index,
                    value: "SHADOW_TEXT".into(),
                },
                NativeScriptCommand::AppendChild {
                    parent_index: child_index,
                    child_index: text_index,
                },
                NativeScriptCommand::AppendChild {
                    parent_index: root_index,
                    child_index,
                },
            ])
            .expect("shadow subtree attaches");

        let (text, truncated) = document.visible_text(1024);
        assert!(!truncated);
        assert!(text.contains("SHADOW_TEXT"));
        assert!(!text.contains("LIGHT_TEXT"));
        let composed = document.composed_children(host);
        assert_eq!(composed.len(), 1);
        assert_eq!(
            document
                .node(composed[0])
                .and_then(|node| node.element_name()),
            Some("span")
        );
    }

    #[test]
    fn custom_element_attach_shadow_exposes_only_open_roots_and_scoped_queries() {
        let runtime = NativeJavaScriptRuntime::new_with_context_id("shadow-dom-fixture")
            .expect("native JavaScript runtime constructs");
        let mut document = NativeDocument::parse(
            "<!doctype html><html><body><x-shadow-fixture>LIGHT_TEXT</x-shadow-fixture></body></html>",
            &NativeEngineLimits::default(),
        )
        .expect("document parses");
        let evaluation = runtime
            .evaluate(
                r##"(() => {
                  class ShadowFixture extends HTMLElement {
                    connectedCallback() {
                      const root = this.attachShadow({ mode: "closed" });
                      const label = document.createElement("span");
                      label.id = "shadow-label";
                      label.textContent = "SHADOW_TEXT";
                      root.appendChild(label);
                      this.savedClosedRoot = root;
                    }
                  }
                  customElements.define("x-shadow-fixture", ShadowFixture);
                  const host = document.querySelector("x-shadow-fixture");
                  const root = host.savedClosedRoot;
                  const openHost = document.createElement("div");
                  document.body.appendChild(openHost);
                  const openRoot = openHost.attachShadow({ mode: "open" });
                  const openLabel = document.createElement("b");
                  openLabel.textContent = "OPEN_SHADOW_TEXT";
                  openRoot.appendChild(openLabel);
                  let duplicateAttachRejected = false;
                  try { openHost.attachShadow({ mode: "open" }); }
                  catch (error) { duplicateAttachRejected = error.name === "NotSupportedError"; }
                  return [
                    typeof Element.prototype.attachShadow === "function",
                    root.mode === "closed",
                    root.host === host,
                    host.shadowRoot === null,
                    root.nodeType === 11,
                    root.querySelector("#shadow-label") !== null,
                    document.querySelector("#shadow-label") === null,
                    openRoot.mode === "open",
                    openHost.shadowRoot === openRoot,
                    openRoot.getElementById("missing") === null,
                    duplicateAttachRejected,
                  ];
                })()"##,
                &document,
                "fixture://shadow-dom.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("custom element attaches a closed shadow root");
        assert_eq!(
            evaluation.value,
            serde_json::json!([
                true, true, true, true, true, true, true, true, true, true, true
            ])
        );
        document
            .apply_script_commands(&evaluation.commands)
            .expect("shadow subtree commands apply");

        let (visible, truncated) = document.visible_text(1024);
        assert!(!truncated);
        assert!(visible.contains("SHADOW_TEXT"));
        assert!(visible.contains("OPEN_SHADOW_TEXT"));
        assert!(!visible.contains("LIGHT_TEXT"));

        let roundtripped = NativeDocument::from_content_wire(
            document.to_content_wire(),
            &NativeEngineLimits::default(),
            2,
        )
        .expect("shadow ownership survives the document wire");
        let (roundtripped_visible, truncated) = roundtripped.visible_text(1024);
        assert!(!truncated);
        assert!(roundtripped_visible.contains("SHADOW_TEXT"));
        assert!(roundtripped_visible.contains("OPEN_SHADOW_TEXT"));
        assert!(!roundtripped_visible.contains("LIGHT_TEXT"));
    }

    #[test]
    fn slot_projection_uses_assigned_light_nodes_and_falls_back_when_empty() {
        let mut document = NativeDocument::parse(
            "<!doctype html><html><body><x-host><span slot='label'>ASSIGNED_TEXT</span></x-host></body></html>",
            &NativeEngineLimits::default(),
        )
        .expect("document parses");
        let host = document
            .find_element(document.root(), "x-host")
            .expect("host exists");
        let root_index = u32::MAX - 8;
        let assigned_slot_index = u32::MAX - 7;
        let assigned_fallback_index = u32::MAX - 6;
        let empty_slot_index = u32::MAX - 5;
        let empty_fallback_index = u32::MAX - 4;
        document
            .apply_script_commands(&[
                NativeScriptCommand::CreateShadowRoot {
                    host_index: host.index(),
                    node_index: root_index,
                    mode: "open".into(),
                },
                NativeScriptCommand::CreateElement {
                    node_index: assigned_slot_index,
                    tag_name: "slot".into(),
                    namespace_uri: None,
                },
                NativeScriptCommand::SetAttribute {
                    node_index: assigned_slot_index,
                    name: "name".into(),
                    value: "label".into(),
                    namespace_uri: None,
                },
                NativeScriptCommand::CreateTextNode {
                    node_index: assigned_fallback_index,
                    value: "ASSIGNED_FALLBACK_MUST_NOT_RENDER".into(),
                },
                NativeScriptCommand::AppendChild {
                    parent_index: assigned_slot_index,
                    child_index: assigned_fallback_index,
                },
                NativeScriptCommand::CreateElement {
                    node_index: empty_slot_index,
                    tag_name: "slot".into(),
                    namespace_uri: None,
                },
                NativeScriptCommand::SetAttribute {
                    node_index: empty_slot_index,
                    name: "name".into(),
                    value: "empty".into(),
                    namespace_uri: None,
                },
                NativeScriptCommand::CreateTextNode {
                    node_index: empty_fallback_index,
                    value: "EMPTY_SLOT_FALLBACK".into(),
                },
                NativeScriptCommand::AppendChild {
                    parent_index: empty_slot_index,
                    child_index: empty_fallback_index,
                },
                NativeScriptCommand::AppendChild {
                    parent_index: root_index,
                    child_index: assigned_slot_index,
                },
                NativeScriptCommand::AppendChild {
                    parent_index: root_index,
                    child_index: empty_slot_index,
                },
            ])
            .expect("shadow slot subtree attaches");

        let (text, truncated) = document.visible_text(1024);
        assert!(!truncated);
        assert!(text.contains("ASSIGNED_TEXT"));
        assert!(!text.contains("ASSIGNED_FALLBACK_MUST_NOT_RENDER"));
        assert!(text.contains("EMPTY_SLOT_FALLBACK"));
    }
}

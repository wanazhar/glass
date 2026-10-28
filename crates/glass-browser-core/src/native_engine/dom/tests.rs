use super::super::css::NativeFontPaletteName;
use super::super::javascript::{
    NativeFrameScriptBinding, NativeJavaScriptRuntime, NativeScriptCommand,
};
use super::super::origin::NativeOrigin;
use super::super::resource_loader::JAVASCRIPT_MIME_TYPE_ESSENCES;
use super::*;

fn transfer_identities(
    document: &NativeDocument,
    root: NativeNodeId,
) -> Vec<NativeNodeTransferIdentity> {
    let mut identities = Vec::new();
    let mut seen = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        assert!(seen.insert(id.index));
        let node = document.raw_node(id).unwrap();
        identities.push(NativeNodeTransferIdentity {
            source_index: id.index,
            destination_temporary_index: u32::MAX
                .saturating_sub(u32::try_from(identities.len()).unwrap()),
        });
        pending.extend(node.children.iter().copied());
        if let Some(content) = node.template_content {
            pending.push(content);
        }
        if let Some(shadow_root) = node.shadow_root {
            pending.push(shadow_root);
        }
    }
    identities
}

fn find_template_element_by_id_if_present(
    document: &NativeDocument,
    template: NativeNodeId,
    expected_id: &str,
) -> Option<NativeNodeId> {
    let content = document
        .raw_node(template)
        .and_then(|node| node.template_content)
        .expect("template must own a contents fragment");
    let mut pending = vec![content];
    while let Some(id) = pending.pop() {
        let Some(node) = document.raw_node(id) else {
            continue;
        };
        if node.element_name().is_some() && node.attribute("id") == Some(expected_id) {
            return Some(id);
        }
        pending.extend(node.children().iter().copied());
        if let Some(contents) = node.template_content {
            pending.push(contents);
        }
    }
    None
}

fn find_template_element_by_id(
    document: &NativeDocument,
    template: NativeNodeId,
    expected_id: &str,
) -> NativeNodeId {
    find_template_element_by_id_if_present(document, template, expected_id)
        .unwrap_or_else(|| panic!("template has no element with id {expected_id:?}"))
}

#[path = "tests/foreign_content.rs"]
mod foreign_content;
#[path = "tests/html_parsing.rs"]
mod html_parsing;
#[path = "tests/layout_style_cache.rs"]
mod layout_style_cache;
#[path = "tests/preload_and_policy.rs"]
mod preload_and_policy;
#[path = "tests/scripts_and_fonts.rs"]
mod scripts_and_fonts;
#[path = "tests/semantic_and_targets.rs"]
mod semantic_and_targets;
#[path = "tests/table_parsing.rs"]
mod table_parsing;
#[path = "tests/tree_mutation.rs"]
mod tree_mutation;

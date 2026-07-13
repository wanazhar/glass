#![no_main]
use glass::browser::dom::{parse_accessibility_tree, parse_dom_tree, project_compact_accessibility};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) {
        let nodes = parse_accessibility_tree(&value);
        let _ = project_compact_accessibility(&nodes, 256);
        let _ = parse_dom_tree(&value);
    }
});

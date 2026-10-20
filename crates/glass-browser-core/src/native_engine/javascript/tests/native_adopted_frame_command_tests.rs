use super::super::config::NativeEngineLimits;
use super::super::origin::NativeOrigin;
use super::*;
use crate::native_engine::dom::NativeNodeTransferIdentity;

#[test]
fn adopted_frame_append_emits_a_native_document_command() {
    let limits = NativeEngineLimits::default();
    let mut source = NativeDocument::parse("<section id='moving'></section>", &limits)
        .expect("source document must parse");
    let mut destination = NativeDocument::parse("<main id='target'></main>", &limits)
        .expect("destination document must parse");
    let source_root = source
        .script_snapshot_for_viewport(1024, Viewport::default())
        .elements
        .into_iter()
        .find(|element| {
            element
                .attributes
                .get("id")
                .is_some_and(|id| id == "moving")
        })
        .expect("source root must exist")
        .node_index;
    let target = destination
        .script_snapshot_for_viewport(1024, Viewport::default())
        .elements
        .into_iter()
        .find(|element| {
            element
                .attributes
                .get("id")
                .is_some_and(|id| id == "target")
        })
        .expect("destination parent must exist")
        .node_index;
    let adopted_alias = u32::MAX - 4096;
    source
        .transfer_script_nodes_to(
            &mut destination,
            &[NativeNodeSubtreeTransfer {
                source_index: source_root,
                identities: vec![NativeNodeTransferIdentity {
                    source_index: source_root,
                    destination_temporary_index: adopted_alias,
                }],
            }],
        )
        .expect("source subtree must transfer");

    let runtime = NativeJavaScriptRuntime::new_with_context_id("native-adopt-command-test")
        .expect("JavaScript runtime must construct");
    let mut events = NativePageEventBatch::default();
    events
        .frame_script_commands
        .push(NativeScriptCommand::AppendChild {
            parent_index: target,
            child_index: adopted_alias,
        });
    let evaluation = runtime
        .evaluate_with_page_events(
            "undefined;",
            &destination,
            "about:blank",
            &NativeOrigin::Opaque,
            Viewport::default(),
            &events,
        )
        .expect("native frame append command must evaluate");
    assert_eq!(
        evaluation.commands,
        vec![NativeScriptCommand::AppendChild {
            parent_index: target,
            child_index: adopted_alias,
        }]
    );
}

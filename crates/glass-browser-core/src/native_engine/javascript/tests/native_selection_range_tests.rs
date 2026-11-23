use super::super::config::NativeEngineLimits;
use super::*;

#[test]
fn javascript_selection_and_range_track_editor_boundaries() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("selection-range-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
        "<html><body><div id='editor'><p id='line'>hello world</p><p id='next'>next</p></div></body></html>",
        &NativeEngineLimits::default(),
    )
    .expect("selection document must parse");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const line = document.getElementById("line");
                    const next = document.getElementById("next");
                    const text = line.firstChild;
                    const range = document.createRange();
                    range.setStart(text, 0);
                    range.setEnd(text, 5);
                    const selection = document.getSelection();
                    selection.removeAllRanges();
                    selection.addRange(range);
                    const forward = [selection.anchorNode === text, selection.focusOffset, selection.toString(), selection.type];
                    selection.setBaseAndExtent(next, 1, text, 6);
                    const backward = [selection.anchorNode === next, selection.focusNode === text, selection.direction, selection.getRangeAt(0).startOffset];
                    selection.collapse(line, 0);
                    return [
                        typeof Range, typeof Selection, window.getSelection() === selection, document.hasFocus(),
                        forward, backward, selection.isCollapsed, selection.rangeCount,
                        range.getBoundingClientRect().width >= 0, range.getClientRects().length,
                    ];
                })()"##,
            &document,
            "fixture://selection-range.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("selection and range must evaluate");

    assert_eq!(
        evaluation.value,
        serde_json::json!([
            "function",
            "function",
            true,
            true,
            [true, 5, "hello", "Range"],
            [true, true, "backward", 6],
            true,
            1,
            true,
            1
        ])
    );
}

#[test]
fn javascript_one_turn_can_queue_more_dom_commands_than_the_effect_cap() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("host-command-budget-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
        "<html><body><div id='root'></div><div id='hidden' tabindex='0' hidden>x</div></body></html>",
        &NativeEngineLimits::default(),
    )
    .expect("host command document must parse");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const root = document.getElementById("root");
                    for (let index = 0; index < 200; index += 1) {
                        root.appendChild(document.createElement("span"));
                    }
                    document.getElementById("hidden").focus();
                    return root.children.length;
                })()"##,
            &document,
            "fixture://host-command-budget.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("building a large editor view in one turn must evaluate");
    assert_eq!(evaluation.value, serde_json::json!(200));
    assert!(evaluation.commands.len() > crate::native_engine::interaction::MAX_NATIVE_EFFECTS);
    document
        .apply_script_commands(&evaluation.commands)
        .expect("a large command batch must commit");
    let root = document.resolve_target("id=root").unwrap();
    assert_eq!(document.node(root).unwrap().children().len(), 200);
}

#[test]
fn script_focus_on_a_hidden_target_is_a_no_op() {
    let mut document = NativeDocument::parse(
        "<html><body><div id='hidden' tabindex='0' hidden>x</div><p id='plain'>y</p></body></html>",
        &NativeEngineLimits::default(),
    )
    .expect("focus document must parse");
    let hidden = document.resolve_target("id=hidden").unwrap();
    let plain = document.resolve_target("id=plain").unwrap();
    let events = document
        .apply_script_commands(&[
            NativeScriptCommand::Focus {
                node_index: hidden.index(),
            },
            NativeScriptCommand::Focus {
                node_index: plain.index(),
            },
        ])
        .expect("focus() on targets that cannot take focus must not fail the batch");
    assert!(events.is_empty());
    assert_ne!(document.focused_node(), hidden);
    assert_ne!(document.focused_node(), plain);
}

use super::*;
use crate::native_engine::NativeEngineLimits;

#[test]
fn host_view_bootstrap_accepts_large_valid_document_with_a_bounded_quickjs_heap() {
    const ELEMENTS: usize = 2_586;
    let mut markup = String::with_capacity(ELEMENTS * 96);
    markup.push_str("<style>.accent { font-weight: 700; }</style><main><section>");
    for index in 0..ELEMENTS {
        let class = if index % 2 == 0 {
            "content-row accent"
        } else {
            "content-row"
        };
        markup.push_str(&format!(
            "<div id='item-{index}' class='{class}' data-index='{index}'>Text item {index}</div>"
        ));
    }
    markup.push_str("</section></main>");

    let runtime = NativeJavaScriptRuntime::new_with_context_id("large-host-view-fixture")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(&markup, &NativeEngineLimits::default())
        .expect("bounded synthetic content page must parse");
    let evaluation = runtime
        .evaluate(
            "[document.body.tagName, document.body.children[0].children[0].children.length, getComputedStyle(document.body.children[0].children[0].children[0]).fontWeight, getComputedStyle(document.body.children[0].children[0].children[1]).fontWeight]",
            &document,
            "fixture://large-host-view.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("large but valid host view must install within the fixed QuickJS memory limit");

    assert_eq!(
        evaluation.value,
        serde_json::json!(["BODY", ELEMENTS, "bold", "normal"])
    );
    let heap = runtime.runtime.memory_usage();
    assert!(heap.memory_used_size > 0);
    assert!(heap.memory_used_size <= 256 * 1024 * 1024);
}

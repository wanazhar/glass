use super::super::config::NativeEngineLimits;
use super::*;

#[test]
fn javascript_scope_selector_queries_respect_owner() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("scope-selector-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<html><head></head><body><main id='main'><button id='button'>Button</button></main></body></html>",
            &NativeEngineLimits::default(),
        )
        .expect("scope selector document must parse");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const main = document.querySelector("#main");
                    const documentScope = document.querySelector(":scope");
                    const body = document.querySelector(":scope > body");
                    const button = main.querySelector(":scope > button");
                    return [
                        documentScope && documentScope.tagName,
                        body && body.tagName,
                        button && button.id,
                        main.querySelector(":scope") === null,
                        main.matches(":scope"),
                        main.querySelectorAll(":scope > button").length,
                    ];
                })()"##,
            &document,
            "fixture://scope-selector.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("scope selector query must evaluate");

    assert_eq!(
        evaluation.value,
        serde_json::json!(["HTML", "BODY", "button", true, true, 1])
    );
}

#[test]
fn javascript_user_validation_selector_queries_follow_native_state() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("user-validation-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
        "<form><input id='required' required></form>",
        &NativeEngineLimits::default(),
    )
    .expect("user validation selector document must parse");
    let required = document
        .resolve_target("id=required")
        .expect("required control must resolve");
    let initial = runtime
        .evaluate(
            r##"(() => {
                    const input = document.querySelector("#required");
                    return [
                        input.matches(":user-invalid"),
                        input.matches(":user-valid"),
                        input.willValidate,
                        input.validity.valid,
                    ];
                })()"##,
            &document,
            "fixture://user-validation-selector.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("initial user validation selectors must evaluate");
    assert_eq!(
        initial.value,
        serde_json::json!([false, false, true, false])
    );

    document.apply_click(required).unwrap();
    let invalid = runtime
        .evaluate(
            r##"document.querySelector("#required").matches(":user-invalid")"##,
            &document,
            "fixture://user-validation-selector.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("user-invalid selector must evaluate");
    assert_eq!(invalid.value, serde_json::json!(true));

    document.apply_type(required, "value").unwrap();
    let valid = runtime
        .evaluate(
            r##"document.querySelector("#required").matches(":user-valid")"##,
            &document,
            "fixture://user-validation-selector.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("user-valid selector must evaluate");
    assert_eq!(valid.value, serde_json::json!(true));
}

#[test]
fn javascript_form_pseudo_selector_queries_follow_native_state() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("form-selector-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
            "<form id='form'><input id='valid' value='ok'><input id='invalid' required><input id='placeholder' placeholder='Name'><input id='readonly' readonly value='locked'><input id='checked' type='checkbox' checked><div id='editable' contenteditable='true'></div></form>",
            &NativeEngineLimits::default(),
        )
        .expect("form selector document must parse");
    let valid = document
        .resolve_target("id=valid")
        .expect("valid control must resolve");
    document.apply_script_focus(valid).unwrap();
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const form = document.querySelector("#form");
                    const valid = document.querySelector("#valid");
                    const invalid = document.querySelector("#invalid");
                    const placeholder = document.querySelector("#placeholder");
                    const readonly = document.querySelector("#readonly");
                    const checked = document.querySelector("#checked");
                    const editable = document.querySelector("#editable");
                    return [
                        valid.matches(":valid"),
                        invalid.matches(":invalid"),
                        invalid.matches(":required"),
                        valid.matches(":optional"),
                        form.matches(":invalid"),
                        form.matches(":focus-within"),
                        valid.matches(":focus"),
                        valid.matches(":focus-visible"),
                        placeholder.matches(":placeholder-shown"),
                        readonly.matches(":read-only"),
                        editable.matches(":read-write"),
                        checked.matches(":default"),
                    ];
                })()"##,
            &document,
            "fixture://form-selector.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("form pseudo selectors must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            true, true, true, true, true, true, true, true, true, true, true, true
        ])
    );
}

#[test]
fn javascript_action_selector_queries_follow_native_state() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("action-selector-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<form><a id='link' href='/target'>Link</a><a id='anchor' name='target'>Anchor</a><input id='low' type='number' value='2' min='3'><input id='mid' type='number' value='5' min='3' max='7'><input id='unbounded' type='number' value='5'><input id='radio-a' type='radio' name='empty'><input id='radio-b' type='radio' name='empty'><input id='radio-checked' type='radio' name='checked' checked><progress id='pending'></progress><progress id='complete' value='1' max='2'></progress></form>",
            &NativeEngineLimits::default(),
        )
        .expect("action selector document must parse");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const link = document.querySelector("#link");
                    const anchor = document.querySelector("#anchor");
                    const low = document.querySelector("#low");
                    const mid = document.querySelector("#mid");
                    const unbounded = document.querySelector("#unbounded");
                    const radioA = document.querySelector("#radio-a");
                    const radioChecked = document.querySelector("#radio-checked");
                    const pending = document.querySelector("#pending");
                    const complete = document.querySelector("#complete");
                    return [
                        link.matches(":link"),
                        link.matches(":any-link"),
                        anchor.matches(":target"),
                        low.matches(":out-of-range"),
                        mid.matches(":in-range"),
                        unbounded.matches(":in-range"),
                        radioA.matches(":indeterminate"),
                        radioChecked.matches(":indeterminate"),
                        pending.matches(":indeterminate"),
                        complete.matches(":indeterminate"),
                    ];
                })()"##,
            &document,
            "fixture://action-selector.test/#target",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("action selectors must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            true, true, true, true, true, false, true, false, true, false
        ])
    );
}
#[test]
fn javascript_structural_selector_queries_follow_native_tree_order() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("structural-selector-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<main><p id='first'></p><span id='span'></span><p id='middle'></p><p id='last'></p><aside id='only-parent'><i id='only'></i></aside></main>",
            &NativeEngineLimits::default(),
        )
        .expect("structural selector document must parse");
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const first = document.querySelector("#first");
                    const middle = document.querySelector("#middle");
                    const last = document.querySelector("#last");
                    const span = document.querySelector("#span");
                    const nthOfType = Array.from(document.querySelectorAll("p:nth-of-type(2)"));
                    const only = document.querySelector("#only");
                    const onlyParent = document.querySelector("#only-parent");
                    const oddChildren = Array.from(document.querySelectorAll("p:nth-child(2n + 1)"));
                    return [
                        first.matches(":first-of-type"),
                        middle.matches(":first-of-type"),
                        span.matches(":first-of-type"),
                        last.matches(":first-of-type"),
                        only.matches(":only-child"),
                        middle.matches(":only-child"),
                        middle.matches(":nth-of-type(2)"),
                        last.matches(":last-of-type"),
                        first.matches(":nth-child(1)"),
                        onlyParent.matches(":nth-last-child(1)"),
                        first.matches("p:nth-child(2n + 1)"),
                        middle.matches("p:nth-child(2n + 1)"),
                        span.matches(":only-of-type"),
                        nthOfType.length === 1 && nthOfType[0].id === "middle",
                        oddChildren.length === 2
                            && oddChildren[0].id === "first"
                            && oddChildren[1].id === "middle",
                    ];
                })()"##,
                &document,
                "fixture://structural-selector.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("structural selectors must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            true, false, true, false, true, false, true, true, true, true, true, true, true, true,
            true
        ])
    );
}
#[test]
fn javascript_has_selector_queries_follow_native_relations() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("has-selector-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<main id='main'><section id='parent'><div id='child'><span id='deep'></span></div><p class='target' id='target'></p></section><section id='empty'></section><a id='anchor'></a><span id='next'></span><span id='later'></span></main>",
            &NativeEngineLimits::default(),
        )
        .expect("has selector document must parse");
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const main = document.querySelector("#main");
                    const parent = document.querySelector("#parent");
                    const child = document.querySelector("#child");
                    const empty = document.querySelector("#empty");
                    const anchor = document.querySelector("#anchor");
                    const sectionsWithDiv = Array.from(document.querySelectorAll("section:has(> div)"));
                    const sectionsWithTarget = Array.from(document.querySelectorAll("section:has(.target)"));
                    return [
                        main.matches(":has(section > .target)"),
                        parent.matches(":has(.target)"),
                        parent.matches(":has(> div)"),
                        parent.matches(":has(> span)"),
                        child.matches(":has(> span)"),
                        empty.matches(":has(span)"),
                        anchor.matches(":has(+ span)"),
                        anchor.matches(":has(~ span)"),
                        sectionsWithDiv.length === 1 && sectionsWithDiv[0].id === "parent",
                        sectionsWithTarget.length === 1 && sectionsWithTarget[0].id === "parent",
                    ];
                })()"##,
                &document,
                "fixture://has-selector.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("has selectors must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, false, true, false, true, true, true, true])
    );
}
#[test]
fn javascript_language_and_direction_selectors_follow_native_inheritance() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("language-direction-selector-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<main id='english' lang='en'><section id='inherit'><p id='french' lang='fr-FR'><span id='french-child'></span></p><p id='german' xml:lang='de-DE'></p><p id='plain'></p><p id='rtl' dir='rtl'><span id='rtl-child'></span></p><p id='auto' dir='auto'></p></section></main>",
            &NativeEngineLimits::default(),
        )
        .expect("language direction selector document must parse");
    let evaluation = runtime
            .evaluate(
                r##"(() => {
                    const english = document.querySelector("#english");
                    const inherit = document.querySelector("#inherit");
                    const french = document.querySelector("#french");
                    const frenchChild = document.querySelector("#french-child");
                    const german = document.querySelector("#german");
                    const plain = document.querySelector("#plain");
                    const rtl = document.querySelector("#rtl");
                    const rtlChild = document.querySelector("#rtl-child");
                    const auto = document.querySelector("#auto");
                    const frenchMatches = Array.from(document.querySelectorAll(":lang(fr)"));
                    const rtlMatches = Array.from(document.querySelectorAll(":dir(rtl)"));
                    return [
                        english.matches(":lang(en)"),
                        inherit.matches(":lang(en)"),
                        plain.matches(":lang(en-*)"),
                        french.matches(":lang(fr)"),
                        french.matches(":lang('fr-FR')"),
                        frenchChild.matches(":lang(fr)"),
                        german.matches(":lang(de)"),
                        english.matches(":lang(es)"),
                        english.matches(":dir(ltr)"),
                        inherit.matches(":dir(ltr)"),
                        rtl.matches(":dir(rtl)"),
                        rtlChild.matches(":dir(rtl)"),
                        rtl.matches(":dir(ltr)"),
                        auto.matches(":dir(ltr)"),
                        auto.matches(":dir(rtl)"),
                        frenchMatches.map((element) => element.id).join(",") === "french,french-child",
                        rtlMatches.length === 2,
                    ];
                })()"##,
                &document,
                "fixture://language-direction-selector.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("language direction selectors must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            true, true, true, true, true, true, true, false, true, true, true, true, false, false,
            false, true, true
        ])
    );
}
#[test]
fn javascript_hover_selector_queries_follow_native_state() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("hover-selector-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse(
            "<main id='main'><section id='parent'><button id='target'>Target</button></section><button id='plain'>Plain</button></main>",
            &NativeEngineLimits::default(),
        )
        .expect("hover selector document must parse");
    let target = document
        .resolve_target("id=target")
        .expect("hover target must resolve");
    document
        .apply_hover(target)
        .expect("native hover state must apply");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const main = document.querySelector("#main");
                    const parent = document.querySelector("#parent");
                    const target = document.querySelector("#target");
                    const plain = document.querySelector("#plain");
                    const hovered = Array.from(document.querySelectorAll(":hover"));
                    return [
                        main.matches(":hover"),
                        parent.matches(":hover"),
                        target.matches(":hover"),
                        plain.matches(":hover"),
                        hovered.map((element) => element.id || element.localName).join(",") === "html,body,main,parent,target",
                    ];
                })()"##,
            &document,
            "fixture://hover-selector.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("hover selectors must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, false, true])
    );
}
#[test]
fn javascript_sibling_combinator_queries_follow_native_tree_order() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("sibling-combinator-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<main><button id='first'></button><button id='second'></button><div id='middle'></div><button id='last'></button></main>",
            &NativeEngineLimits::default(),
        )
        .expect("sibling combinator document must parse");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const first = document.querySelector("#first");
                    const second = document.querySelector("#second");
                    const middle = document.querySelector("#middle");
                    const last = document.querySelector("#last");
                    const adjacent = Array.from(document.querySelectorAll("#first + button"));
                    const following = Array.from(document.querySelectorAll("#first ~ *"));
                    const childAdjacent = document.querySelector("main > #first + button");
                    return [
                        second.matches("#first + button"),
                        last.matches("#first + button"),
                        middle.matches("#first ~ div"),
                        last.matches("#first ~ button"),
                        childAdjacent && childAdjacent.id === "second",
                        adjacent.length === 1 && adjacent[0].id === "second",
                        following.map((element) => element.id).join(",") === "second,middle,last",
                        document.querySelectorAll("button + button").length === 1,
                    ];
                })()"##,
            &document,
            "fixture://sibling-combinator.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("sibling combinators must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([true, false, true, true, true, true, true, true])
    );
}
#[test]
fn javascript_nth_child_of_selector_queries_use_filtered_siblings() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("nth-of-selector-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<main><p id='first' class='item'></p><span id='skip'></span><p id='second' class='item'></p><p id='third' class='item'></p><p id='other' class='other'></p><p id='fourth' class='item'></p></main>",
            &NativeEngineLimits::default(),
        )
        .expect("nth-of selector document must parse");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const first = document.querySelector("#first");
                    const second = document.querySelector("#second");
                    const third = document.querySelector("#third");
                    const skip = document.querySelector("#skip");
                    const fourth = document.querySelector("#fourth");
                    const oddItems = Array.from(
                        document.querySelectorAll(":nth-child(odd of .item)")
                    );
                    const lastItems = Array.from(
                        document.querySelectorAll("p:nth-last-child(2 of .item)")
                    );
                    let syntaxError = false;
                    try {
                        document.querySelectorAll(":nth-child(2 of)");
                    } catch (error) {
                        syntaxError = error instanceof SyntaxError;
                    }
                    return [
                        second.matches("p:nth-child(2 of .item)"),
                        third.matches("p:nth-last-child(2 of .item)"),
                        fourth.matches(":nth-child(2n of .item)"),
                        !skip.matches(":nth-child(2 of .item)"),
                        first.matches("p:nth-child(2 of .item, .other)") === false,
                        oddItems.map((element) => element.id).join(",") === "first,third",
                        lastItems.length === 1 && lastItems[0].id === "third",
                        syntaxError,
                    ];
                })()"##,
            &document,
            "fixture://nth-of-selector.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("nth-of selectors must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true, true, true, true])
    );
}
#[test]
fn javascript_attribute_selectors_honor_namespaces_and_case_modifiers() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("attribute-selector-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<main><div id='exact' data-label='MiXeD'></div><svg id='icon'><use id='use'></use></svg></main>",
            &NativeEngineLimits::default(),
        )
        .expect("attribute selector document must parse");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const exact = document.querySelector("#exact");
                    const use = document.querySelector("#use");
                    use.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", "#one");
                    let modifierError = false;
                    let namespaceError = false;
                    try {
                        document.querySelectorAll("[data-label='mixed' q]");
                    } catch (error) {
                        modifierError = error instanceof SyntaxError;
                    }
                    try {
                        document.querySelectorAll("[unknown|href]");
                    } catch (error) {
                        namespaceError = error instanceof SyntaxError;
                    }
                    return [
                        exact.matches("[data-label='mixed' i]"),
                        !exact.matches("[data-label='mixed' s]"),
                        exact.matches("[data-label='MiXeD' s]"),
                        document.querySelectorAll("[data-label='mixed' i]").length === 1,
                        use.matches("[xlink|href='#one']"),
                        use.matches("[*|href='#one']"),
                        !use.matches("[|href='#one']"),
                        !use.matches("[href='#one']"),
                        modifierError,
                        namespaceError,
                    ];
                })()"##,
            &document,
            "fixture://attribute-selector.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("attribute selectors must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true, true, true, true, true, true])
    );
}
#[test]
fn javascript_nested_functional_selectors_match_nested_arguments() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("nested-selector-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<main><div id='first' class='card'><span class='target'></span></div><div id='muted' class='card muted'><span></span></div><div id='last' class='card'><span class='target'></span></div></main>",
            &NativeEngineLimits::default(),
        )
        .expect("nested selector document must parse");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const first = document.querySelector("#first");
                    const muted = document.querySelector("#muted");
                    const last = document.querySelector("#last");
                    let syntaxError = false;
                    try {
                        document.querySelectorAll("div:not(:is(.card)");
                    } catch (error) {
                        syntaxError = error instanceof SyntaxError;
                    }
                    return [
                        first.matches(":is(.card:not(.muted))"),
                        !muted.matches(":is(.card:not(.muted))"),
                        !first.matches(":not(:has(> span.target))"),
                        muted.matches(":not(:has(> span.target))"),
                        last.matches(":where(.card:not(.muted))"),
                        document.querySelectorAll("div:not(:is(.muted, #first))").length === 1,
                        document.querySelector("div:is(.card, :not(.other))").id === "first",
                        syntaxError,
                    ];
                })()"##,
            &document,
            "fixture://nested-selector.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("nested functional selectors must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true, true, true, true])
    );
}
#[test]
fn javascript_attribute_selectors_use_html_default_case_rules() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("attribute-default-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<form id='form'><input id='field'><div id='state'></div><div id='custom' data-value='MiXeD'></div></form>",
            &NativeEngineLimits::default(),
        )
        .expect("attribute default document must parse");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    const form = document.querySelector("#form");
                    const field = document.querySelector("#field");
                    const state = document.querySelector("#state");
                    const custom = document.querySelector("#custom");
                    form.setAttribute("method", "POST");
                    form.setAttribute("enctype", "APPLICATION/X-WWW-FORM-URLENCODED");
                    field.setAttribute("type", "TeXt");
                    state.setAttribute("dir", "RTL");
                    state.setAttribute("contenteditable", "TRUE");
                    return [
                        form.matches("[method=post]"),
                        form.matches("[enctype=application/x-www-form-urlencoded]"),
                        field.matches("[type=text]"),
                        !field.matches("[type=text s]"),
                        state.matches("[dir=rtl]"),
                        !state.matches("[dir=rtl s]"),
                        state.matches("[contenteditable=true]"),
                        !custom.matches("[data-value=mixed]"),
                        custom.matches("[data-value=mixed i]"),
                        !form.matches("[method=post s]"),
                    ];
                })()"##,
            &document,
            "fixture://attribute-default.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("default attribute selectors must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([true, true, true, true, true, true, true, true, true, true])
    );
}

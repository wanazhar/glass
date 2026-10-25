use super::*;
use crate::native_engine::NativeEngineLimits;

#[test]
fn host_view_covers_representative_html_svg_mathml_and_custom_element_categories() {
    let markup = r##"<!doctype html>
      <html><head><title>local element contract</title><meta charset="utf-8">
        <link rel="stylesheet" href="data:text/css,body%7Bcolor%3A%23000%7D">
        <style>article { color: #123456 }</style>
      </head><body>
        <header><nav><a href="#main">home</a></nav></header>
        <main id="main"><article><h1>Heading</h1><h2>Subheading</h2>
          <h3>Third</h3><h4>Fourth</h4><h5>Fifth</h5><h6>Sixth</h6>
          <section><p><strong>strong</strong> <em>em</em> <small>small</small>
            <s>strike</s> <cite>cite</cite> <q>quote</q> <dfn>term</dfn>
            <abbr title="application">app</abbr> <ruby>ruby<rp>(</rp><rt>reading</rt><rp>)</rp></ruby>
            <data value="7">seven</data> <time datetime="2026-10-25">today</time>
            <code>code</code> <var>v</var> <samp>output</samp> <kbd>key</kbd>
            H<sub>2</sub>O x<sup>2</sup> <i>italic</i> <b>bold</b> <u>underline</u>
            <mark>mark</mark> <bdi>name</bdi> <bdo dir="rtl">text</bdo>
            <span>span</span><br><wbr><ins>inserted</ins><del>deleted</del></p>
            <hr><pre>preformatted</pre><blockquote>block quote</blockquote>
            <ol><li>ordered</li></ol><ul><li>unordered</li></ul>
            <menu><li>menu</li></menu><dl><dt>term</dt><dd>definition</dd></dl>
            <figure><figcaption>caption</figcaption></figure><address>address</address>
            <search>search</search>
          </section><aside>aside</aside>
          <table id="data"><caption>data</caption><colgroup><col></colgroup>
            <thead><tr><th scope="col">name</th></tr></thead>
            <tbody><tr><td>row</td></tr></tbody><tfoot><tr><td>end</td></tr></tfoot>
          </table>
          <form id="form"><fieldset><legend>details</legend>
            <label for="email">email</label><input id="email" value="reader@example.test">
            <button type="button">submit</button><select><optgroup label="group">
              <option value="one">one</option></optgroup></select>
            <datalist id="hints"><option value="hint"></datalist>
            <textarea>message</textarea><output>result</output><progress value="2" max="3"></progress>
            <meter value="0.5"></meter></fieldset></form>
          <picture><source srcset="data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs=" type="image/gif">
            <img alt="pixel" src="data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs="></picture>
          <audio controls><source src="data:audio/wav;base64,">audio</audio>
          <video controls><track kind="captions" src="data:text/vtt,WEBVTT"></video>
          <canvas width="32" height="16"></canvas><iframe title="frame"></iframe>
          <embed type="text/plain"><object><param name="x" value="y"></object>
          <map name="map"><area shape="rect" coords="0,0,1,1" href="#main" alt="area"></map>
          <svg viewBox="0 0 10 10"><title>vector</title><rect width="10" height="10"></rect></svg>
          <math><mi>x</mi><mo>+</mo><mn>1</mn></math>
          <details><summary>summary</summary><div>disclosure</div></details>
          <dialog open>dialog</dialog><template id="template"><slot></slot></template>
          <x-card data-kind="custom">custom</x-card>
        </article></main><footer>footer</footer>
      </body></html>"##;
    let document = NativeDocument::parse(markup, &NativeEngineLimits::default())
        .expect("representative local element-category page must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("element-category-fixture")
        .expect("native JavaScript runtime must construct");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
              const required = [
                "header", "nav", "main", "article", "section", "aside", "footer",
                "h1", "h2", "h3", "h4", "h5", "h6", "p", "strong", "em", "small",
                "s", "cite", "q", "dfn", "abbr", "ruby", "rp", "rt", "data", "time",
                "code", "var", "samp", "kbd", "sub", "sup", "i", "b", "u", "mark",
                "bdi", "bdo", "span", "br", "wbr", "ins", "del", "hr", "pre",
                "blockquote", "ol", "ul", "menu", "li", "dl", "dt", "dd", "figure",
                "figcaption", "address", "search", "table", "caption", "colgroup", "col",
                "thead", "tbody", "tfoot", "tr", "th", "td", "form", "fieldset", "legend",
                "label", "input", "button", "select", "optgroup", "option", "datalist",
                "textarea", "output", "progress", "meter", "picture", "source", "img", "audio",
                "video", "track", "canvas", "iframe", "embed", "object", "param", "map", "area",
                "details", "summary", "dialog", "template", "x-card"
              ];
              const actual = Array.from(document.querySelectorAll("body *"), node => node.localName);
              return {
                missing: required.filter(tag => !actual.includes(tag)),
                forms: document.forms.length,
                tableRows: document.querySelectorAll("#data tr").length,
                inputValue: document.getElementById("email").value,
                svgWidth: document.querySelector("svg rect").getAttribute("width"),
                mathNumber: document.querySelector("math mn").textContent,
                templateSlot: document.getElementById("template").content.querySelector("slot").localName
              };
            })()"##,
            &document,
            "fixture://element-categories.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("host view must expose representative HTML, SVG, MathML, and custom elements");

    assert_eq!(
        evaluation.value,
        serde_json::json!({
            "missing": [],
            "forms": 1,
            "tableRows": 3,
            "inputValue": "reader@example.test",
            "svgWidth": "10",
            "mathNumber": "1",
            "templateSlot": "slot"
        })
    );
}

#[test]
fn repeated_large_host_views_stay_within_the_bounded_quickjs_heap() {
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
    for _ in 0..6 {
        let evaluation = runtime
            .evaluate(
                "[document.body.tagName, document.body.children[0].children[0].children.length, getComputedStyle(document.body.children[0].children[0].children[0]).fontWeight, getComputedStyle(document.body.children[0].children[0].children[1]).fontWeight]",
                &document,
                "fixture://large-host-view.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("repeated large host views must install within the fixed QuickJS memory limit");

        assert_eq!(
            evaluation.value,
            serde_json::json!(["BODY", ELEMENTS, "bold", "normal"])
        );
    }
    let heap = runtime.runtime.memory_usage();
    assert!(heap.memory_used_size > 0);
    assert!(heap.memory_used_size <= 256 * 1024 * 1024);
}

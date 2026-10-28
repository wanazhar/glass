use super::*;
use crate::native_engine::NativeEngineLimits;

#[test]
fn base64_fast_path_decodes_bytes_and_rejects_bad_padding() {
    let document = NativeDocument::parse("<body></body>", &NativeEngineLimits::default())
        .expect("base64 decoder fixture must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("base64-fast-path")
        .expect("native JavaScript runtime must construct");
    let decoded = runtime
        .evaluate(
            "Array.from(atob('AAECA/8='), (character) => character.charCodeAt(0))",
            &document,
            "fixture://base64-fast-path.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("valid base64 must decode");
    assert_eq!(decoded.value, serde_json::json!([0, 1, 2, 3, 255]));

    let invalid = runtime.evaluate(
        "atob('AA=A')",
        &document,
        "fixture://base64-fast-path.test/",
        &NativeOrigin::Opaque,
        Viewport::default(),
    );
    assert!(invalid.is_err(), "interior base64 padding must fail");
}

#[test]
fn host_view_cache_key_tracks_resource_projection_invalidations() {
    let viewport = Viewport::default();
    let mut document = NativeDocument::parse(
        "<img id='animated' src='/animated.gif'>",
        &NativeEngineLimits::default(),
    )
    .expect("native image fixture must parse");
    let original_revision = document.revision();
    let original_key = NativeHostViewCacheKey::new(
        &document,
        viewport,
        true,
        NativePoint { x: 0, y: 0 },
        &BTreeMap::new(),
    );
    let image_id = document
        .element_node_ids()
        .find(|id| {
            document
                .node(*id)
                .is_some_and(|node| node.element_name() == Some("img"))
        })
        .expect("fixture image must be an attached element");
    let source = document.image_current_src(image_id, viewport);
    document
        .mark_image_load(image_id.index(), source.clone(), viewport)
        .expect("image request state must be accepted");
    let animation = crate::native_engine::image::NativeImage::with_frames(
        1,
        1,
        vec![
            crate::native_engine::image::NativeImageFrame {
                delay_ms: 10,
                pixels: vec![0; 4],
            },
            crate::native_engine::image::NativeImageFrame {
                delay_ms: 10,
                pixels: vec![1; 4],
            },
        ],
        Some(0),
    )
    .expect("bounded animation fixture must be valid");
    document
        .set_image_resource(image_id.index(), source, animation)
        .expect("decoded image resource must be accepted");

    assert_eq!(document.revision(), original_revision);
    let invalidated_key = NativeHostViewCacheKey::new(
        &document,
        viewport,
        true,
        NativePoint { x: 0, y: 0 },
        &BTreeMap::new(),
    );
    assert_ne!(invalidated_key, original_key);
}

#[test]
fn dynamically_created_img_link_and_script_keep_reflected_url_and_policy_properties() {
    let document = NativeDocument::parse(
        "<body><main id='mount'></main></body>",
        &NativeEngineLimits::default(),
    )
    .expect("dynamic element fixture must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("dynamic-reflected-elements")
        .expect("native JavaScript runtime must construct");
    let evaluation = runtime
        .evaluate(
            r#"(() => {
                const image = document.createElement("img");
                const link = document.createElement("link");
                const script = document.createElement("script");
                image.src = "/pixel.png";
                link.href = "/site.css";
                script.src = "/app.js";
                script.referrerPolicy = "no-referrer";
                return [
                    image.src, image.getAttribute("src"), image.complete,
                    image.currentSrc, image.naturalWidth, image.naturalHeight,
                    link.href, link.getAttribute("href"),
                    script.src, script.getAttribute("src"),
                    script.referrerPolicy, script.getAttribute("referrerpolicy"),
                ];
            })()"#,
            &document,
            "https://fixture.test/article",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("dynamic reflected properties must be available before static resource tags");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            "https://fixture.test/pixel.png",
            "/pixel.png",
            false,
            "https://fixture.test/pixel.png",
            0,
            0,
            "https://fixture.test/site.css",
            "/site.css",
            "https://fixture.test/app.js",
            "/app.js",
            "no-referrer",
            "no-referrer"
        ])
    );
}

#[test]
fn initial_inner_html_and_text_content_follow_projected_child_identity() {
    let document = NativeDocument::parse(
        "<body><div id='root'><p>Hello <span>world</span><!-- hidden --></p><script>const marker = 1;</script><select><option>Option &amp; text</option></select></div></body>",
        &NativeEngineLimits::default(),
    )
    .expect("content projection fixture must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("derived-initial-content")
        .expect("native JavaScript runtime must construct");
    let evaluation = runtime
        .evaluate(
            r#"(() => {
                const root = document.getElementById("root");
                const paragraph = root.children[0];
                const script = root.children[1];
                const option = root.children[2].children[0];
                const replacement = document.createElement("div");
                replacement.innerHTML = "<b>replacement</b> &amp; text";
                return [
                    root.textContent,
                    paragraph.innerHTML,
                    paragraph.outerHTML,
                    script.textContent,
                    option.textContent,
                    option.value,
                    replacement.innerHTML,
                    replacement.textContent,
                ];
            })()"#,
            &document,
            "https://fixture.test/content",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("content getters must derive from the installed host child tree");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            "Hello worldconst marker = 1;Option & text",
            "Hello <span>world</span><!-- hidden -->",
            "<p>Hello <span>world</span><!-- hidden --></p>",
            "const marker = 1;",
            "Option & text",
            "Option & text",
            "<b>replacement</b> &amp; text",
            "replacement & text"
        ])
    );
    assert!(
        evaluation
            .commands
            .iter()
            .any(|command| matches!(command, NativeScriptCommand::SetInnerHtml { .. }))
    );
    assert!(
        !evaluation
            .commands
            .iter()
            .any(|command| matches!(command, NativeScriptCommand::AppendChild { .. }))
    );
}

#[test]
fn same_origin_frame_content_getters_derive_from_projected_child_nodes() {
    let parent = NativeDocument::parse(
        "<body><iframe id='frame'></iframe></body>",
        &NativeEngineLimits::default(),
    )
    .expect("frame parent fixture must parse");
    let child = NativeDocument::parse(
        "<body><article id='article'><p>Frame <span>content</span></p><!-- ignored --></article></body>",
        &NativeEngineLimits::default(),
    )
    .expect("frame child fixture must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("frame-lazy-child-content")
        .expect("native JavaScript runtime must construct");
    runtime.set_frame_script_bindings(vec![NativeFrameScriptBinding {
        node_index: parent
            .element_node_ids()
            .find(|id| {
                parent
                    .node(*id)
                    .is_some_and(|node| node.element_name() == Some("iframe"))
            })
            .expect("frame parent element must exist")
            .index(),
        frame_id: "lazy-content-child".into(),
        url: "https://fixture.test/child".into(),
        origin: "https://fixture.test".into(),
        generation: 1,
        revision: 1,
        same_origin: true,
        document: child.script_snapshot_for_viewport(1 << 20, Viewport::default()),
        children: Vec::new(),
    }]);
    let evaluation = runtime
        .evaluate(
            "(() => { const article = document.getElementById('frame').contentDocument.getElementById('article'); return [article.textContent, article.innerHTML, article.firstChild.textContent]; })()",
            &parent,
            "https://fixture.test/parent",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("same-origin frame content getters must reflect the projected child tree");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            "Frame content",
            "<p>Frame <span>content</span></p><!-- ignored -->",
            "Frame content"
        ])
    );
}

#[test]
fn repeated_host_views_reuse_decoded_canvas_image_resources_and_repair_public_map() {
    let mut document = NativeDocument::parse(
        "<img id='source' src='/pixel.png'><canvas id='target' width='1' height='1'></canvas>",
        &NativeEngineLimits::default(),
    )
    .expect("canvas image fixture must parse");
    let image_id = document
        .element_node_ids()
        .find(|id| {
            document
                .node(*id)
                .is_some_and(|node| node.element_name() == Some("img"))
        })
        .expect("fixture image must exist");
    let source = document.image_current_src(image_id, Viewport::default());
    document
        .mark_image_load(image_id.index(), source.clone(), Viewport::default())
        .expect("image request state must be accepted");
    document
        .set_image_resource(
            image_id.index(),
            source.clone(),
            crate::native_engine::image::NativeImage::new(1, 1, vec![17, 34, 51, 255]),
        )
        .expect("decoded fixture image must be accepted");

    let runtime = NativeJavaScriptRuntime::new_with_context_id("canvas-image-cache")
        .expect("native JavaScript runtime must construct");
    let first = runtime
        .evaluate(
            "(() => { const canvas = document.getElementById('target'); canvas.getContext('2d').drawImage(document.getElementById('source'), 0, 0); const resource = globalThis.__glassCanvasImageResources.values().next().value; const result = [globalThis.__glassCanvasImageResources.size, Array.isArray(resource.pixels), canvas.__glassCanvasSurface.pixels.slice(0, 4)]; globalThis.__glassHeldCanvasImageResources = globalThis.__glassCanvasImageResources; resource.pixels[0] = 99; globalThis.__glassCanvasImageResources.clear(); globalThis.__glassCanvasImageResources.set(-1, null); return result; })()",
            &document,
            "fixture://canvas-cache.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("initial canvas image resources must install");
    assert_eq!(first.value, serde_json::json!([1, true, [17, 34, 51, 255]]));

    let second = runtime
        .evaluate(
            "(() => { const canvas = document.getElementById('target'); canvas.getContext('2d').clearRect(0, 0, 1, 1); canvas.getContext('2d').drawImage(document.getElementById('source'), 0, 0); const resource = globalThis.__glassCanvasImageResources.values().next().value; const result = [globalThis.__glassCanvasImageResources.size, globalThis.__glassCanvasImageResources.has(-1), Array.isArray(resource.pixels), resource.pixels[0], globalThis.__glassHeldCanvasImageResources === globalThis.__glassCanvasImageResources, canvas.__glassCanvasSurface.pixels.slice(0, 4)]; globalThis.__glassCanvasSurfaces.clear(); return result; })()",
            &document,
            "fixture://canvas-cache.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("unchanged image resource snapshot must reuse its decoded cache");
    assert_eq!(
        second.value,
        serde_json::json!([1, false, true, 17, true, [17, 34, 51, 255]])
    );

    let canvas_commit = second
        .commands
        .iter()
        .find_map(|command| match command {
            NativeScriptCommand::CanvasCommit {
                node_index,
                width,
                height,
                pixels_base64,
                origin_clean,
            } => Some((
                *node_index,
                *width,
                *height,
                pixels_base64.clone(),
                *origin_clean,
            )),
            _ => None,
        })
        .expect("drawing the canvas must emit a persisted canvas resource");
    document
        .apply_script_canvas(
            canvas_commit.0,
            canvas_commit.1,
            canvas_commit.2,
            &canvas_commit.3,
            canvas_commit.4,
            &std::collections::BTreeMap::new(),
        )
        .expect("canvas resource must be accepted by the document");

    document
        .mark_image_load(image_id.index(), source.clone(), Viewport::default())
        .expect("replacement image request state must be accepted");
    document
        .set_image_resource(
            image_id.index(),
            source,
            crate::native_engine::image::NativeImage::new(1, 1, vec![201, 202, 203, 255]),
        )
        .expect("replacement image resource must be accepted");
    let updated = runtime
        .evaluate(
            "(() => { const canvas = document.getElementById('target'); const persistedResource = globalThis.__glassCanvasResources.values().next().value; const persistedResourcePixels = persistedResource && persistedResource.pixels.slice(0, 4); const persistedCanvasPixels = canvas.__glassCanvasSurface.pixels.slice(0, 4); canvas.getContext('2d').clearRect(0, 0, 1, 1); canvas.getContext('2d').drawImage(document.getElementById('source'), 0, 0); return [globalThis.__glassCanvasImageResources.size, persistedResourcePixels, persistedCanvasPixels, canvas.__glassCanvasSurface.pixels.slice(0, 4)]; })()",
            &document,
            "fixture://canvas-cache.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("changed image resource snapshot must invalidate its decoded cache");
    assert_eq!(
        updated.value,
        serde_json::json!([
            1,
            [17, 34, 51, 255],
            [17, 34, 51, 255],
            [201, 202, 203, 255]
        ])
    );
}

#[test]
fn large_decoded_image_resource_survives_base64_host_install() {
    let mut document = NativeDocument::parse(
        "<img id='source' src='/large.png'><canvas id='target' width='256' height='256'></canvas>",
        &NativeEngineLimits::default(),
    )
    .expect("large canvas image fixture must parse");
    let image_id = document
        .element_node_ids()
        .find(|id| {
            document
                .node(*id)
                .is_some_and(|node| node.element_name() == Some("img"))
        })
        .expect("large fixture image must exist");
    let source = document.image_current_src(image_id, Viewport::default());
    document
        .mark_image_load(image_id.index(), source.clone(), Viewport::default())
        .expect("large image request state must be accepted");
    document
        .set_image_resource(
            image_id.index(),
            source,
            crate::native_engine::image::NativeImage::new(256, 256, vec![23; 256 * 256 * 4]),
        )
        .expect("large decoded image must be accepted");

    let runtime = NativeJavaScriptRuntime::new_with_context_id("large-image-base64")
        .expect("native JavaScript runtime must construct");
    let result = runtime
        .evaluate(
            "(() => { const image = document.getElementById('source'); const canvas = document.getElementById('target'); canvas.getContext('2d').drawImage(image, 0, 0); const pixels = canvas.__glassCanvasSurface.pixels; return [image.complete, image.naturalWidth, pixels[0], pixels[pixels.length - 1]]; })()",
            &document,
            "fixture://large-image-base64.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("large decoded image must install and paint");
    assert_eq!(result.value, serde_json::json!([true, 256, 23, 23]));
}

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
    // Eight and a half thousand mixed elements plus text nodes and scaffolding
    // exercise the 16,384-node parser ceiling without raising that limit.
    const ELEMENTS: usize = 8_500;
    const NESTED_WRAPPERS: usize = 20;
    let mut markup = String::with_capacity(ELEMENTS * 96);
    markup.push_str("<style>.accent { font-weight: 700; }</style><main><section>");
    for _ in 0..NESTED_WRAPPERS {
        markup.push_str("<div class='nested'>");
    }
    for index in 0..ELEMENTS {
        let class = if index % 2 == 0 { "row accent" } else { "row" };
        let tag = [
            "div", "span", "p", "a", "button", "article", "section", "img",
        ][index % 8];
        if tag == "img" {
            markup.push_str(&format!(
                "<img id='item-{index}' class='{class}' data-index='{index}' aria-label='image {index}' style='color:#{index:06x}' src='/image/{index}.png' alt='preview'>"
            ));
        } else if tag == "a" {
            markup.push_str(&format!(
                "<a id='item-{index}' class='{class}' data-index='{index}' aria-label='link {index}' style='color:#{index:06x}' href='/item/{index}'>x</a>"
            ));
        } else {
            markup.push_str(&format!(
                "<{tag} id='item-{index}' class='{class}' data-index='{index}' aria-label='row {index}' style='color:#{index:06x}'>x</{tag}>"
            ));
        }
    }
    for _ in 0..NESTED_WRAPPERS {
        markup.push_str("</div>");
    }
    markup.push_str("</section></main>");

    let runtime = NativeJavaScriptRuntime::new_with_context_id("large-host-view-fixture")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(&markup, &NativeEngineLimits::default())
        .expect("bounded synthetic content page must parse");
    for install in 0..6 {
        let (source, expected) = if install == 0 {
            (
                "(() => { const first = document.getElementById('item-0'); globalThis.__glassHeldHostElement = first; globalThis.__glassHeldHostElements = globalThis.__glassHostElements; globalThis.__glassHeldElementMapSize = globalThis.__glassHostElements.size; globalThis.__glassHeldNodeMapSize = globalThis.__glassHostNodes.size; globalThis.__glassHostElements.set(-1, null); globalThis.__glassHostNodes.clear(); globalThis.__glassHeldDocument = document; globalThis.__glassDocumentObserver = new MutationObserver(() => {}); globalThis.__glassDocumentObserver.observe(document, { attributes: true, subtree: true }); first.__glassReuseMarker = 42; return [document.body.tagName, document.getElementsByClassName('row').length, getComputedStyle(first).fontWeight, getComputedStyle(document.getElementById('item-1')).fontWeight, Object.prototype.hasOwnProperty.call(globalThis, '__glassInstallNativeHostView'), document.documentElement.parentNode === document, document.documentElement.getRootNode() === document]; })()",
                serde_json::json!(["BODY", ELEMENTS, "bold", "normal", false, true, true]),
            )
        } else {
            (
                "[document.body.tagName, document.getElementsByClassName('row').length, getComputedStyle(document.getElementById('item-0')).fontWeight, getComputedStyle(document.getElementById('item-1')).fontWeight, Object.prototype.hasOwnProperty.call(globalThis, '__glassInstallNativeHostView'), globalThis.__glassHeldHostElement === document.getElementById('item-0'), globalThis.__glassHeldHostElement.__glassReuseMarker, document.documentElement.parentNode === document, document.documentElement.getRootNode() === document, globalThis.__glassHeldDocument !== document, globalThis.__glassHeldHostElements !== globalThis.__glassHostElements, globalThis.__glassHostElements.size === globalThis.__glassHeldElementMapSize, globalThis.__glassHostNodes.size === globalThis.__glassHeldNodeMapSize, !globalThis.__glassHostElements.has(-1), globalThis.__glassHostNodes.size > 0]",
                serde_json::json!([
                    "BODY", ELEMENTS, "bold", "normal", false, true, 42, true, true, true, true,
                    true, true, true, true
                ]),
            )
        };
        let evaluation = runtime
            .evaluate(
                source,
                &document,
                "fixture://large-host-view.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("repeated large host views must install within the fixed QuickJS memory limit");

        assert_eq!(evaluation.value, expected);
    }
    let observer_evaluation = runtime
        .evaluate(
            "(() => { document.body.setAttribute('data-observed', 'yes'); const records = globalThis.__glassDocumentObserver.takeRecords(); return [document.documentElement.parentNode === document, document.documentElement.getRootNode() === document, records.length, records[0] && records[0].target === document.body]; })()",
            &document,
            "fixture://large-host-view.test/observer",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("document subtree observers must survive an unchanged host bootstrap");
    assert_eq!(
        observer_evaluation.value,
        serde_json::json!([true, true, 1, true])
    );
    let changed_markup = markup.replacen(
        "id='item-0' class='row accent'",
        "id='item-0' class='row changed'",
        1,
    );
    let changed_document = NativeDocument::parse(&changed_markup, &NativeEngineLimits::default())
        .expect("changed fixture document must parse");
    let changed_evaluation = runtime
        .evaluate(
            "(() => { const first = document.getElementById('item-0'); return [first.className, getComputedStyle(first).fontWeight, globalThis.__glassHeldHostElement === first, globalThis.__glassHeldHostElement.__glassReuseMarker, globalThis.__glassHeldHostElements === globalThis.__glassHostElements, globalThis.__glassHeldHostNodes === globalThis.__glassHostNodes]; })()",
            &changed_document,
            "fixture://large-host-view.test/changed",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("a changed document key must perform a complete host refresh");
    assert_eq!(
        changed_evaluation.value,
        serde_json::json!(["row changed", "normal", true, 42, false, false])
    );
    let second_runtime = NativeJavaScriptRuntime::new_with_context_id("second-large-host-view")
        .expect("second native JavaScript runtime must construct");
    let second_runtime_evaluation = second_runtime
        .evaluate(
            "[document.getElementsByClassName('row').length, document.getElementById('item-0').id]",
            &document,
            "fixture://large-host-view.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("a runtime without a private state cache must receive the full cached document snapshot");
    assert_eq!(
        second_runtime_evaluation.value,
        serde_json::json!([ELEMENTS, "item-0"])
    );
    let heap = runtime.runtime.memory_usage();
    assert!(heap.memory_used_size > 0);
    assert!(heap.memory_used_size <= 256 * 1024 * 1024);
}

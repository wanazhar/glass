use super::*;
use crate::native_engine::NativeEngineLimits;

#[test]
fn shared_element_methods_and_lazy_surfaces_preserve_identity_and_updates() {
    let first_document = NativeDocument::parse(
        "<main id='root'><section id='section' data-stage='first'><a id='link' href='/one'>one</a></section><svg><rect id='vector' width='2'></rect></svg></main>",
        &NativeEngineLimits::default(),
    )
    .expect("first shared-method fixture must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("shared-element-methods")
        .expect("native JavaScript runtime must construct");

    let first = runtime
        .evaluate(
            r##"(() => {
              const root = document.querySelector("#root");
              const section = document.querySelector("#section");
              const link = document.querySelector("#link");
              const vector = document.querySelector("#vector");
              const classes = link.classList;
              const style = link.style;
              const dataset = link.dataset;
              classes.add("featured");
              style.setProperty("color", "red");
              dataset.articleId = "shared";
              section.__glassRefresh({
                ...section.__glassEntry,
                attributes: { ...section.__glassEntry.attributes, "data-stage": "second" },
              });
              link.__glassRefresh({
                ...link.__glassEntry,
                attributes: { ...link.__glassEntry.attributes, href: "/two" },
              });
              vector.__glassRefresh({
                ...vector.__glassEntry,
                attributes: { ...vector.__glassEntry.attributes, width: "3" },
              });
              return {
                methodPairs: [
                  root.getAttribute === section.getAttribute,
                  section.getAttribute === link.getAttribute,
                  link.getAttribute === vector.getAttribute,
                  link.matches === vector.matches,
                ],
                names: link.getAttributeNames().sort(),
                matches: link.matches("a[href='/two']"),
                closest: link.closest("section") === section,
                scopedQuery: section.querySelector("a") === link,
                scopedQueryCount: section.querySelectorAll("a").length,
                tagCollectionCount: root.getElementsByTagName("a").length,
                stage: section.getAttribute("data-stage"),
                href: link.getAttribute("href"),
                vectorWidth: vector.getAttribute("width"),
                surfaces: [
                  link.classList === classes,
                  link.style === style,
                  link.dataset === dataset,
                  link.getAttribute("class"),
                  link.getAttribute("style"),
                  link.getAttribute("data-article-id"),
                  Object.prototype.hasOwnProperty.call(link, "classList"),
                  Object.prototype.hasOwnProperty.call(link, "style"),
                  Object.prototype.hasOwnProperty.call(link, "dataset"),
                ],
              };
            })()"##,
            &first_document,
            "fixture://shared-element-methods.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("shared prototype methods must work on the initial host view");
    assert_eq!(
        first.value,
        serde_json::json!({
            "methodPairs": [true, true, true, true],
            "names": ["class", "data-article-id", "href", "id", "style"],
            "matches": true,
            "closest": true,
            "scopedQuery": true,
            "scopedQueryCount": 1,
            "tagCollectionCount": 1,
            "stage": "second",
            "href": "/two",
            "vectorWidth": "3",
            "surfaces": [true, true, true, "featured", "color: red;", "shared", false, false, false],
        })
    );
}

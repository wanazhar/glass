---
id: native-engine-browser-771
scope: glass-browser/native-image-map-hit-testing
status: in-progress
depends-on: [native-engine-browser-770]
---

# Glass native-engine browser slice 771: image-map hit testing and activation

## Objective

Make native `<img usemap>` regions first-class point-click targets, with the
same cancellation, link-default, accessibility, and navigation owners as
ordinary native hyperlinks.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for browser completion.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) requires
  image-map areas to work through native browser actions, not only appear in
  serialized DOM.
- The [HTML Standard: Image maps](https://html.spec.whatwg.org/multipage/image-maps.html)
  defines map association, area geometry, stacking, and pointer targeting.
  Coordinates are interpreted in CSS pixels relative to the displayed image
  after CSS width/height stretching; earlier areas are topmost.
- The [HTML Standard: Links](https://html.spec.whatwg.org/multipage/links.html#following-hyperlinks)
  gives `area[href]` hyperlink activation the ordinary link default-action
  contract.
- [WAI technique H24](https://www.w3.org/WAI/WCAG21/Techniques/html/H24.html)
  identifies each linked area's `alt` as its text alternative.
- Image resources, image dimensions, document hit testing, click cancellation,
  hyperlink navigation, and same-origin frame ownership already have native
  state owners; the implementation should extend those owners instead of
  introducing a parallel image-map action path.

## Contract

- Resolve an `img` element's current `usemap` reference to the matching `map`
  in the same document tree. One map may be associated with multiple rendered
  images; hit testing uses the particular image box under the pointer. DOM
  mutations to `usemap`, map names, area order, shape, or coordinates take
  effect on the next action without stale geometry.
- Support the HTML `default`, `rect`, `circle`, and `poly` shape states,
  including coordinate-list normalization, empty-shape behavior, rectangle
  endpoint normalization, and shape-boundary cases. Scale coordinates against
  the image's displayed width and height; browser zoom and CSS/SVG transforms
  do not change the map's coordinate system. A missing or invalid `shape` uses
  the HTML rectangle default.
- Areas layer in map tree order with the first matching area topmost. A
  topmost area without `href` still receives the pointer event and does not
  activate a lower overlapping hyperlink. If no area covers the point, the
  image remains the hit target and ordinary image behavior is unchanged.
- Point actions dispatch the normal cancelable click event to the selected
  `<area>` and use the existing event path/revision owner. `preventDefault()`
  suppresses link activation. The live post-listener `href`, `target`,
  `download`, modifier state, URL policy, and opener/download owners remain
  authoritative.
- Linked areas expose the semantic `link` role and `alt` text as their
  accessible name. They can be resolved and activated through ordinary Glass
  semantic actions as well as coordinate clicks. Non-linked areas are not
  exposed as actionable links.
- Local documents, HTTP(S) content-process documents, and same-origin frame
  documents share the same association, geometry, event, and navigation rules.

## Boundaries

This slice covers client-side `<img usemap>` image maps. Server-side `ismap`,
non-HTML image-map mechanisms, complete keyboard sequential-focus behavior for
areas, complete WPT coverage, cross-platform certification, remote CI, and
issue #40 completion remain separate gates. Do not claim complete image-map
conformance from this slice alone.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-771.md`

## Verification

- Add deterministic geometry tests for each shape, coordinate normalization,
  CSS-stretched images, overlapping areas/tree order, dead-area interception,
  default coverage, missing associations, and live DOM mutation.
- Verify click target/event order and cancellation, current `href`, ordinary
  navigation, `_blank`, download, modifier context creation, and accessible
  link name/action behavior.
- Cover local, HTTP(S) content-process, and same-origin-frame image maps using
  real point actions through the public native backend.
- Run `cargo fmt --all -- --check`, `git diff --check`, then
  `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  and the affected `glass-dev` target check before focused image-map tests.
- Do not run workspace/all-targets tests, remote CI, or clean Cargo artifacts
  for this bounded slice.

---
id: native-engine-browser-691
scope: native-engine/browser/namespace-aware-html-void-elements
status: complete
depends-on: [native-engine-browser-690]
---

# Objective

Apply HTML void-element behavior only to elements in the HTML namespace across
document parsing, Rust fragment commit, same-turn JavaScript projection, XHR
HTML-document parsing, and serialization.

## Contract

- In HTML parsing, a local name in the HTML void-element set does not push an
  HTML-namespace element onto the open-element stack.
- A foreign SVG/MathML element with the same local name remains an ordinary
  open element unless its start tag has the self-closing flag. Its descendants
  and matching end tag must preserve the foreign element's namespace and
  parentage.
- An explicitly self-closing foreign element is popped regardless of its local
  name; the following element is its sibling.
- HTML serialization applies void syntax only to HTML-namespace elements.
  Same-named foreign elements serialize their children and closing tags.
- `innerHTML` always follows fragment parsing and replacement semantics for
  its target. A target's local name being an HTML void name does not silently
  discard parsed fragment children.
- Existing HTML void-element parsing and serialization behavior remains
  unchanged. Do not add the separate foreign-content breakout start-tag
  dispatcher in this slice.
- No general HTML tree-construction or foreign-content conformance claim.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-690.md`
- [WHATWG HTML foreign-content parsing](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inforeign)
- [WHATWG `innerHTML` setter](https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html#the-innerhtml-property)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-691.md`

## Verification

- Add an exact-tree fixture using non-breakout void names in SVG and MathML:
  an unclosed foreign element with a nested child, plus a self-closing foreign
  element followed by a sibling. Also cover an HTML void element followed by
  an HTML sibling.
- Compare namespace and parentage across document parsing, Rust fragment
  commit, same-turn JavaScript projection, and XHR HTML-document parsing.
- Verify foreign same-named elements serialize nested descendants and closing
  tags; retain existing HTML void serialization behavior.
- Verify setting `innerHTML` on an HTML void-named target commits its parsed
  child in both the JS projection and Rust document.
- Run the locked `glass-browser` library/test-target check before the focused
  void-element/HTML parser tests.
- Run formatting, diff whitespace, and maintainer documentation validators.
- Do not claim remote CI, publication, browser conformance, or issue #40
  completion from this slice.

## Outcome

HTML void-element stack behavior and HTML serialization now check both the
element's local name and its HTML namespace. Same-named SVG/MathML elements
retain nested descendants unless explicitly self-closing. Fragment `innerHTML`
is parsed and committed even when the target is named as an HTML void element.
Document parsing, Rust fragment commit, same-turn JavaScript projection, and
XHR HTML-document parsing agree on exact namespace and parentage.

- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- The exact four-route namespace/parentage and serialization regression — 1
  passed.
- The focused `html_` parser regression batch, excluding the separate
  uncommitted CSS-selector draft test — 28 passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Maintainer documentation gates passed: 1,319 Markdown documents with zero
  current-claim failures; 93 guides and 19 contracts; 346 MCP tools (101
  browser-only), 17 examples, 22 public modules, and 15 shortcut help keys/63
  documentation markers.
- Local validation only. No remote CI, publication, or issue #40 completion is
  claimed.

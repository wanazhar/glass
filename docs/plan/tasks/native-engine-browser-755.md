---
id: native-engine-browser-755
scope: glass-browser/document-import-node
status: planned
depends-on: [native-engine-browser-754]
---

# Glass native-engine browser slice 755: import nodes into a target Document

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) remains the
  authority for native browser completion.
- Slice 754 implements shallow/deep template cloning for `cloneNode()`, but
  the live top-level and same-origin-frame `Document` objects do not expose
  `importNode()`.
- The [DOM Standard import algorithm](https://dom.spec.whatwg.org/#dom-document-importnode)
  clones into the target Document. A boolean selects shallow/deep cloning; an
  `ImportNodeOptions` dictionary selects subtree copying with `selfOnly`, and
  may supply a custom-element registry.
- The [HTML template algorithms](https://html.spec.whatwg.org/multipage/scripting.html#the-template-element)
  keep template contents in a separate fragment owned by the target Document's
  associated inert template document. Deeply imported templates copy their
  content into that destination fragment, including nested templates.

## Objective

Expose target-document-aware `Document.importNode()` in native live top-level,
same-origin-frame, and inert template-owner Documents. Keep the source node and
its owner unchanged, return an independent detached copy, and preserve HTML
template fragment and inert-owner boundaries.

## Contract

- Accept the native node kinds already supported by the native clone path:
  elements (including namespaced attributes), attributes, text, comments,
  document types, and document fragments. The imported root and every copied
  descendant use the requested target Document, except that descendants copied
  into an HTML template's `.content` use that target Document's associated
  inert template contents owner Document.
- `importNode(node)` and `importNode(node, false)` copy only the root;
  `importNode(node, true)` copies ordinary descendants and deep template
  contents. For an options dictionary, `selfOnly` defaults to false, so `{}`
  copies the subtree and `{ selfOnly: true }` copies only the root.
- Imported templates have independent, stable `.content` fragments. Deep
  imports recursively copy nested template content; no source node, attribute,
  or fragment is shared, and content does not become an ordinary template
  child. Template descendants use the target's inert template owner document,
  not the source's.
- Provide the method on top-level, same-origin-frame, and inert template-owner
  Documents. Preserve existing detached-node, frame-command, and value
  boundaries; importing does not attach or mutate the source.
- A Document input throws `NotSupportedError`; invalid non-Node inputs throw
  `TypeError`; unsupported node kinds and non-null
  `customElementRegistry` options fail explicitly instead of being ignored.
- This slice does not implement `adoptNode()`, custom-element registries or
  callbacks, Shadow DOM, all DOM node kinds, full DOM conformance, remote CI,
  cross-platform certification, or issue #40 completion.

## Tradeoffs

The existing `cloneNode()` and the new `importNode()` share node-copying
semantics but select different target Documents. This avoids a second template
copy implementation while ensuring an import never inherits the source
document's inert template owner. Non-null custom-element registries are
rejected with a typed DOM exception until the native registry implementation
can apply the standard cloning hooks.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-755.md`

## Verification

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --tests --locked --quiet`
- Focused process-backed top-level and same-origin-frame import regressions.
- Maintainer documentation inventory, route, current-claim, depth, shortcut,
  and live-coverage gates.
- `git diff --check`.
- Evidence is local Linux only; broader DOM conformance, remote CI, and
  cross-platform certification remain issue-level gates.

## Evidence

Pending implementation and verification.

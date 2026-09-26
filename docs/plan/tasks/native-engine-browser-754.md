---
id: native-engine-browser-754
scope: glass-browser/template-content-clone-node
status: complete
depends-on: [native-engine-browser-753]
---

# Glass native-engine browser slice 754: clone template contents

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) remains the
  authority for native browser completion.
- Slices 752 and 753 preserve template contents as separate native fragments
  in top-level and same-origin frame documents. The shared `cloneNode()` path
  currently deep-clones only ordinary node children; because template content
  is not an ordinary child, a deep-cloned template loses its contents.
- The [DOM Standard cloning algorithm](https://dom.spec.whatwg.org/#concept-node-clone)
  invokes specification-defined cloning steps before ordinary subtree
  recursion. The [HTML template cloning steps](https://html.spec.whatwg.org/multipage/scripting.html#the-template-element)
  copy template-content children only for a deep clone, into the copy's own
  template-content fragment and its associated inert owner document.

## Objective

Implement the HTML template cloning step in Glass's shared `cloneNode()` path
so shallow and deep clones preserve separate content fragments in the live
top-level and same-origin frame realms.

## Contract

- `template.cloneNode(false)` returns an independent template with a distinct,
  stable, empty `.content` fragment.
- `template.cloneNode(true)` recursively clones the source template's content
  children into the copy's `.content`, including nested template fragments.
  No content node or fragment is shared with the source, and contents do not
  become ordinary children of the template element.
- The cloned fragment and its descendants use the inert template-contents
  owner document associated with the clone's node document. Mutating the clone
  does not mutate the source.
- Apply the same behavior to parsed templates in the top-level document and
  in same-origin frame documents. Preserve fragment/native command ownership
  and existing node/depth bounds.
- This slice does not implement cross-document `importNode()` or `adoptNode()`
  template algorithms, custom-element cloning hooks, full DOM conformance,
  remote CI, cross-platform certification, or issue #40 completion.

## Tradeoffs

The shared clone implementation remains the single owner of node copying and
adds the template-specific copy step before ordinary child recursion. The
copied subtree retains fragment identity and inert ownership without making
template contents visible to normal document traversal. Cross-document copy
and adoption policies remain separate because they can change owner-document
identity.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-754.md`

## Verification

- `cargo fmt --all -- --check` passed.
- `cargo check -p glass-browser --tests --locked --quiet` passed.
- `cargo test --quiet -p glass-browser --test native_engine template_content
  --locked -- --test-threads=1` passed: both top-level and same-origin-frame
  process-backed regressions, with 787 unrelated tests filtered out.
- `git diff --check` passed.
- Release-documentation truth passed over 1,382 Markdown documents with zero
  current-claim failures; depth passed for 93 current guides and 19 contracts;
  shortcut inventory passed for 15 implementation keys and 63 markers; live
  coverage passed for 346 full-product MCP tools (101 browser-only), 17
  examples, and 22 public modules.
- Evidence is local Linux only. `importNode()`, `adoptNode()`, complete DOM
  conformance, remote CI, and cross-platform certification remain separate
  issue #40 gates.

## Evidence

The shared clone path applies template cloning steps before ordinary child
recursion. Shallow and deep clones retain separate stable `.content` fragments;
deep clones recursively copy nested content, and the destination fragment is
associated with the source template's inert owner document. Top-level and
same-origin-frame process-backed regressions verify clone independence and
persistence across realm refresh. Scoped `glass-browser` check and both
focused process tests pass on local Linux. All four maintainer documentation
gates pass for the current checkout. `importNode()`, `adoptNode()`, complete
WPT/GCWP conformance, remote CI, and cross-platform certification remain open;
this slice does not complete issue #40.

---
id: native-engine-browser-776
scope: glass-browser/light-dom-inert-focus
status: completed
depends-on: [native-engine-browser-775]
---

# Glass native-engine browser slice 776: light-DOM inert focus

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [Core Web Profile](../native-engine-browser-profile.md) requires
  focus behavior to account for an element's inert state.
- The [HTML Standard inert-subtree rules](https://html.spec.whatwg.org/multipage/interaction.html#inert-subtrees)
  exclude inert focus targets and descendants from focus behavior. The
  [`HTMLElement.inert` IDL attribute](https://html.spec.whatwg.org/multipage/dom.html#the-inert-attribute)
  reflects the content attribute.
- Slice 775 established disabled custom-element focus behavior; this slice
  adds the bounded light-DOM inert portion without claiming full inertness.

## Objective

Reflect `HTMLElement.inert` through the live boolean content attribute and
prevent focus operations from targeting elements under an inert light-DOM
ancestor.

## Contract

- Rust-owned sequential and programmatic focus checks the candidate and its
  DOM ancestors for a no-namespace `inert` attribute.
- Image-map focus checks the anchor as well as the area so an inert anchor
  cannot receive focus through its area.
- The shared JavaScript `HTMLElement.inert` property reads and writes the
  boolean content attribute. Property and attribute mutations affect the next
  focus operation.
- Local and content-process JavaScript focus projections do not optimistically
  dispatch focus events or update local focus for an inert target.
- Local and HTTP(S) same-origin-frame process-backed regressions cover
  reflection, inherited inertness, dynamic changes, programmatic focus, and Tab
  ordering.

## Boundaries and tradeoffs

This is focus-specific and limited to ancestor traversal in the light DOM. It
does not implement flat-tree/shadow descendants, modal-dialog top-layer
exceptions, pointer or hit testing, editing, selection, find-in-page, or
accessibility effects of inert subtrees. It is not complete inert or focus
conformance.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-776.md`

## Verification

Passed:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --locked --quiet` (existing unused parser
  item warnings only)
- `cargo test -p glass-browser --test native_engine --locked --quiet -- inert_focus_state --test-threads=1`
  (2 local and HTTP(S) same-origin-frame regressions; 52.74 seconds)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-776.json`
  (1,404 Markdown documents; zero current-claim failures)
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
  (1,404 Markdown files)
- `git diff --check`

Remote CI, Web Platform Tests, cross-platform certification, and issue #40
completion remain open.

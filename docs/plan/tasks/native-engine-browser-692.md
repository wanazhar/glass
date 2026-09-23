---
id: native-engine-browser-692
scope: native-engine/browser/foreign-content-breakout-start-tags
status: complete
depends-on: [native-engine-browser-691]
---

# Objective

Implement WHATWG foreign-content breakout dispatch for qualifying start tags
in all four native HTML parser routes: document navigation, Rust `innerHTML`
commit, same-turn JavaScript fragment projection, and XHR HTML-document
parsing.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-691.md`
- [WHATWG HTML foreign-content parsing](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inforeign)

## Contract

- In foreign content, these start-tag names break out and are reprocessed by
  HTML tree-construction rules: `b`, `big`, `blockquote`, `body`, `br`,
  `center`, `code`, `dd`, `div`, `dl`, `dt`, `em`, `embed`, `h1`, `h2`, `h3`,
  `h4`, `h5`, `h6`, `head`, `hr`, `i`, `img`, `li`, `listing`, `menu`, `meta`,
  `nobr`, `ol`, `p`, `pre`, `ruby`, `s`, `small`, `span`, `strong`, `strike`,
  `sub`, `sup`, `table`, `tt`, `u`, `ul`, and `var`.
- `font` breaks out only when the token has an attribute named `color`, `face`,
  or `size`; values do not affect this test.
- Pop foreign open elements only until the current node is an HTML element or
  an HTML/MathML integration point. Do not cross an integration point.
- Reprocess the same start tag under HTML rules. In a fragment whose context
  element is foreign, preserve the fragment target as the insertion parent but
  model the synthetic HTML fragment root after breakout, so projected and
  committed children receive the same namespace.
- HTML tree construction ignores the self-closing flag for every HTML-
  namespace element, including children created at integration points and
  breakout tokens reprocessed as HTML. Non-void HTML elements remain open;
  HTML void elements still follow HTML void behavior. Explicitly self-closing
  foreign-namespace elements retain their existing behavior.
- `area` is not a breakout name and remains governed by the namespace-aware
  behavior from slice 691. Integration-point behavior from slice 689 remains
  intact.
- Foreign end-tag breakout (`</br>` and `</p>`), SVG/MathML attribute
  adjustment, other foreign-content tokens, and general HTML parser
  conformance remain separate work. This slice must not claim full
  foreign-content or HTML parser conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-692.md`

## Verification

- Unit-test the complete breakout-name set and the conditional `font`
  attribute-name rule, including nearby non-breakout names and `font` without
  a trigger attribute.
- Compare exact namespace and parentage across document parsing, Rust
  fragment commit, same-turn JavaScript projection, and XHR HTML-document
  parsing. Cover a nested SVG breakout, a MathML breakout, conditional and
  non-conditional `font`, integration points, non-breakout `area`, and
  self-closing HTML syntax on both a reprocessed non-void element and an
  integration-point child. Keep explicitly self-closing foreign elements
  closed.
- Verify `innerHTML` on a foreign fragment context preserves the target while
  parsing breakout children in the HTML namespace in both the JS projection
  and Rust commit.
- Run the locked `glass-browser` library/test-target check before the focused
  parser regressions; then run the scoped HTML parser batch.
- Run formatting, whitespace, and maintainer documentation validators.
- Do not claim remote CI, release, publication, browser conformance, or issue
  #40 completion from this slice.

## Local evidence

- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- `cargo test -p glass-browser --lib foreign_ --locked --quiet` passed 4/4.
- `cargo test -p glass-browser --lib html_ --locked --quiet -- --skip javascript_attribute_selectors_use_html_default_case_rules` passed 31/31.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Release-truth, documentation-depth, TUI-shortcut, and documentation-coverage
  validators passed. Coverage found 1,320 Markdown files, 346 MCP tools (101
  browser-only), 17 examples, and 22 public modules.
- Remote CI was not run. General HTML/foreign-content conformance, end-tag
  breakout, and foreign attribute adjustment remain open.

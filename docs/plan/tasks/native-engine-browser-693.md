---
id: native-engine-browser-693
scope: native-engine/browser/foreign-content-breakout-end-tags
status: done
depends-on: [native-engine-browser-692]
---

# Objective

Implement WHATWG foreign-content breakout handling for `</br>` and `</p>` in
all four native HTML parser routes: document parsing, Rust `innerHTML` commit,
same-turn JavaScript fragment projection, and XHR HTML-document parsing.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-692.md`
- [WHATWG HTML foreign-content parsing](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inforeign)

## Contract

- A foreign-content end tag named `br` or `p` is a breakout, including when
  the current foreign node is an HTML/MathML integration point. Pop open foreign
  elements only until the current node is an HTML element, a MathML text
  integration point, or an HTML integration point. Clear formatting markers
  belonging to popped nodes and do not pop the fragment target sentinel.
- Reprocess the token under the active HTML insertion mode. When the in-body
  rules apply, `</br>` behaves as a `br` start tag with no attributes and
  inserts an HTML-namespace void element. For `</p>`, close an HTML `p` in
  button scope; if none is in scope, reconstruct active formatting, insert a
  paragraph, and immediately close it. Do not let a foreign element with local
  name `p` or `br` masquerade as an HTML scope target.
- Run existing table-mode end-tag consumers before the in-body fallback. In
  column-group mode, `</br>` and `</p>` are ignored; table, row, and cell
  contexts retain their insertion-mode recovery behavior.
- In a foreign fragment context, preserve the target as the insertion parent
  and switch the synthetic fragment insertion context to HTML after breakout.
  Projection and Rust commit must agree with document and XHR parser routes.
- Other foreign end tags retain their current bounded behavior. This task does
  not claim complete foreign-content or HTML parser conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-693.md`

## Verification

- Test `</br>` conversion, `</p>` with an in-scope HTML paragraph, and `</p>`
  with no paragraph in button scope (empty paragraph insertion), including a
  paragraph hidden behind a button boundary and ordinary HTML input.
- Compare exact namespace and parentage for nested SVG and MathML content,
  HTML and MathML integration-point boundaries, non-breakout foreign end tags,
  table column-group end-tag behavior, and a foreign fragment target across
  all four parser routes.
- Ensure self-closing foreign parsing and the start-tag breakout behavior from
  slice 692 remain unchanged.
- Run the locked `glass-browser` library/test-target check before focused
  parser regressions, then the scoped HTML parser batch.
- Run formatting, whitespace, and maintainer documentation validators.
- Do not claim remote CI, release, publication, browser conformance, or issue
  #40 completion from this slice.

## Local evidence

- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- `cargo test -p glass-browser --lib foreign_ --locked --quiet` passed 6/6;
  the focused fragment rerun passed 1/1.
- `cargo test -p glass-browser --lib html_ --locked --quiet -- --skip javascript_attribute_selectors_use_html_default_case_rules` passed 33/33. The skip preserves a separate user-owned selector draft.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Release-truth validation passed over 1,321 Markdown files with zero
  current-claim failures; documentation depth passed for 93 guides/19
  contracts; shortcuts passed for 15 implementation keys/63 markers; coverage
  passed for 346 MCP tools, 17 examples, and 22 public modules.
- Remote CI was not run. General HTML/foreign-content conformance and issue
  #40 completion remain open.

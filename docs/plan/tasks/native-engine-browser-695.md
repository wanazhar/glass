---
id: native-engine-browser-695
scope: native-engine/browser/foreign-attribute-adjustment
status: done
depends-on: [native-engine-browser-694]
---

# Objective

Implement WHATWG foreign-content attribute adjustment consistently in all
Glass HTML parser routes, preserving DOM-visible names and namespace identity.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-694.md`
- [WHATWG HTML foreign-content parsing](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inforeign)

## Contract

- Apply MathML's `definitionurl` → `definitionURL` adjustment and every entry
  in the current WHATWG SVG attribute-name adjustment table.
- Apply the complete WHATWG foreign-attribute table: the seven `xlink:*`
  names, `xml:lang`, `xml:space`, `xmlns`, and `xmlns:xlink`. Preserve each
  adjusted qualified name, prefix, local name, and namespace URI in the native
  DOM state and JavaScript projections.
- Run adjustments only for tokens inserted through the applicable
  foreign-content rules, including `<svg>` and `<math>` entry points. A
  prefix-looking attribute on an HTML-namespace element remains
  unnamespaced.
- Keep tokenizer duplicate handling first-wins and ASCII-case-insensitive
  before applying case/namespace adjustments.
- Expose the same attribute identity through `getAttribute`, `getAttributeNS`,
  `hasAttributeNS`, attribute-node properties, cloning/projection, and HTML
  serialization where those surfaces are already implemented.
- Keep existing parser insertion, namespace, integration-point, breakout,
  and end-tag behavior unchanged. Do not reinterpret attributes created or
  changed by script APIs; this task concerns parser-created tokens only.
- Direct document parsing, committed `innerHTML`, same-turn JavaScript
  fragment projection, and XHR `text/html` parsing must agree.
- This is a parser increment, not a claim of general HTML or browser
  conformance, and does not close issue #40.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-695.md`

## Verification

- Add exact assertions for representative and edge mappings, including
  `viewbox` → `viewBox`, `preserveaspectratio` → `preserveAspectRatio`,
  MathML `definitionurl`, XLink `href`, XML `lang`, and both XMLNS forms.
- Assert `prefix`, `localName`, `namespaceURI`, qualified attribute name,
  namespaced lookup/removal identity, and serialization.
- Assert a prefix-looking `xlink:href` on an HTML element has no namespace and
  duplicate raw attributes retain only the first value.
- Exercise all four routes with equivalent fixtures and retain existing
  foreign-content regressions.
- Run `cargo check -p glass-browser --lib --tests --locked --quiet` before
  focused parser tests; run the foreign and scoped HTML parser batches after
  the coherent implementation.
- Run formatting, whitespace, release-documentation truth,
  documentation-depth, and shortcut validators. Run full docs inventory/link
  coverage if the change reaches a debug-binary inventory.
- Record local versus remote evidence separately. Do not claim remote CI,
  release, cross-platform certification, or issue #40 completion from this
  slice.

## Local evidence

The locked package check passed:
`cargo check -p glass-browser --lib --tests --locked --quiet`.
The foreign-focused batch passed 9/9:
`cargo test -p glass-browser --lib foreign_ --locked --quiet`.
The scoped HTML batch passed 34/34:
`cargo test -p glass-browser --lib html_ --locked --quiet -- --skip javascript_attribute_selectors_use_html_default_case_rules`.
Formatting and whitespace checks passed, as did release-documentation truth,
documentation-depth, and shortcut validation. Full documentation coverage was
not run because no CLI, MCP, example, or Rust-module inventory changed and no
debug CLI binaries were built. Remote CI was not run. Issue #40 remains open;
general parser conformance remains incomplete.

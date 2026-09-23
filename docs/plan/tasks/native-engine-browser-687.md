---
id: native-engine-browser-687
scope: native-engine/browser/html-input-newline-preprocessing
status: complete
depends-on: [native-engine-browser-686]
---

# Objective

Normalize HTML parser input newlines before tokenization: each CRLF pair and
each lone CR becomes one LF. Apply the same behavior to document parsing, Rust
`innerHTML` commit, same-turn JavaScript fragment projection, and XHR
`responseType="document"` parsing. Text, raw text, RCDATA, comments, and
attribute values must all observe the normalized stream.

Do not change existing source/command size accounting: where a byte limit is
enforced, it remains measured against the submitted source before
normalization. Preserve original-source byte offsets in Rust parse errors.
Do not globally rewrite U+0000: its HTML handling depends on tokenizer state
and tree-construction context. This slice does not implement that behavior or
claim general tokenizer/parser conformance.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-686.md`
- [WHATWG preprocessing the input stream](https://html.spec.whatwg.org/multipage/parsing.html#preprocessing-the-input-stream)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-687.md`

## Verification

- Add one route-parity fixture covering CRLF and lone CR in ordinary text,
  attributes, comments, script raw text, and RCDATA, checking exact LF output
  through direct document parsing, Rust fragment commit, same-turn projection,
  and XHR HTML-document parsing.
- Add focused coverage that Rust parse errors after CRLF normalization still
  report the original input byte offset.
- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- `cargo test -p glass-browser --lib html_ --locked --quiet -- --skip
  javascript_attribute_selectors_use_html_default_case_rules` passed 24/24 in
  9.63 s. The skipped CSS-selector default-case test is outside this parser
  slice.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Maintainer validators passed: release truth covered 1,315 Markdown files,
  83 current documents, 63 previous-version hits, 1,429 semantic audit hits,
  and zero current-claim failures; depth covered 93 routed/audited current
  guides and 19 substantive contracts; coverage covered 1,315 Markdown files,
  346 full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules; shortcut inventory covered 15 implementation help keys and 63
  documentation markers.
- Do not claim remote CI, publication, browser conformance, or completion of
  issue #40 from this slice.

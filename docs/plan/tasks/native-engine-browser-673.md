---
id: native-engine-browser-673
scope: native-engine/browser/html-tree-builder
status: done
depends-on: [native-engine-browser-672]
---

# Native Engine Browser Slice 673: Bounded Table Foster Parenting

## Objective

Implement the bounded HTML foster-parent insertion behavior in
`NativeDocument::parse`, as described in
`docs/architecture/native-engine.md` and informed by the
[WHATWG HTML parsing algorithm](https://html.spec.whatwg.org/multipage/parsing.html#the-in-table-insertion-mode).
When the current node is an HTML-namespace `table`, `tbody`, `tfoot`, `thead`,
or `tr`, insert non-ASCII-whitespace text runs and ordinary start tags before
the nearest open HTML `table` in its parent. Keep ASCII-whitespace-only runs
and comments in the table. Push foster-inserted non-void elements onto the
open-element stack, so their descendants remain parented to those elements.
Do not apply the rule to SVG/MathML elements or to table-special tokens, which
retain the existing bounded parser behavior.

This slice does not change the script `innerHTML` fragment parser or the
JavaScript XHR `responseType="document"` parser, and it does not claim complete
WHATWG insertion modes, table section/row normalization, foreign-content
recovery, parser-route parity, or general HTML parser conformance.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- [WHATWG HTML Standard, tree construction](https://html.spec.whatwg.org/multipage/parsing.html#the-in-table-insertion-mode)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-673.md`

## Verification

- `cargo check -p glass-browser --lib --tests --locked` — passed; finished in
  46.85s with no warnings.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib browser::native_engine::dom::tests --locked` — 35 passed, 0 failed.
- `rustfmt --edition 2024 --check crates/glass-browser/src/browser/native_engine/dom.rs` — passed.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-native-673-release-documentation-final.json` — 1301 Markdown documents, 0 current-claim failures.
- `python3 scripts/check-documentation-depth.py` — 93 guides and 19 substantive contracts validated.
- `python3 scripts/check-tui-shortcuts.py` — 15 implementation keys and 63 documentation markers validated.
- `python3 scripts/check-documentation-coverage.py` — 1301 Markdown files, 346 MCP tools, 17 examples, and 22 public modules validated.
- `rustfmt --edition 2024 --check crates/glass-browser/src/browser/native_engine/dom.rs` and `git diff --check` — passed.

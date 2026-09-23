---
id: native-engine-browser-684
scope: native-engine/browser/html-table-row-context-ignored-end-tags
status: done
depends-on: [native-engine-browser-683]
---

# Objective

Ignore the in-table-mode end-tag set `body`, `caption`, `col`, `colgroup`, and
`html` while an actual HTML table row or row group is in scope and no cell is
open. Consume these tokens before generic name-based matching can pop an
ancestor. Preserve the dedicated handling for active cells, tables,
captions, and column groups. Keep document parsing, Rust `innerHTML` commit,
same-turn JavaScript detached-fragment projection, and XHR HTML document
parsing aligned.

This remains a bounded insertion-mode increment, not full HTML parser
conformance.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-683.md`
- [WHATWG in-table-body insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-intablebody)
- [WHATWG in-row insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inrow)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-684.md`

## Verification

- `rustfmt --edition 2024 crates/glass-browser/src/browser/native_engine/dom.rs crates/glass-browser/src/browser/native_engine/javascript.rs` — passed.
- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- `cargo test -p glass-browser --lib javascript_xhr_and_fragments_ignore_row_end_tags_without_cells --locked --quiet` — 1 passed; checks all five ignored end tags in row and row-group contexts across direct parsing, Rust fragment commit, same-turn projection, and XHR HTML parsing.
- `cargo test -p glass-browser --lib table --locked --quiet` — 45 passed.
- `python3 scripts/check-release-documentation.py --require-previous-version --report <temporary report>` — passed; 1,312 Markdown documents, 83 current documents, and zero current-claim failures.
- `python3 scripts/check-documentation-depth.py` — passed; 93 current guides and 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py` — passed; 15 implementation help keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py` — passed; 1,312 Markdown files, 346 MCP tools, 17 examples, and 22 public modules.
- `rustfmt --check --edition 2024 crates/glass-browser/src/browser/native_engine/dom.rs crates/glass-browser/src/browser/native_engine/javascript.rs` and `git diff --check` — passed.
- Remote CI, push, release, publication, and full browser conformance are not
  claimed by this local task.

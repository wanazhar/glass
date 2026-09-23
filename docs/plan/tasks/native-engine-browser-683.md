---
id: native-engine-browser-683
scope: native-engine/browser/html-table-in-cell-ignored-end-tags
status: done
depends-on: [native-engine-browser-682]
---

# Objective

Apply the HTML in-cell insertion-mode rule that ignores end tags `body`,
`caption`, `col`, `colgroup`, and `html` while an actual HTML `td` or `th` is
in table scope. Consume those tokens before generic name-based stack matching
can close an ancestor. Keep document parsing, Rust `innerHTML` commit,
same-turn JavaScript detached-fragment projection, and XHR HTML document
parsing aligned.

This is a bounded parser rule, not general HTML tree-construction or
active-formatting-list conformance.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- [WHATWG in-cell insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incell)
- `docs/plan/tasks/native-engine-browser-682.md`

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-683.md`

## Verification

- `rustfmt --edition 2024 crates/glass-browser/src/browser/native_engine/dom.rs crates/glass-browser/src/browser/native_engine/javascript.rs` — passed.
- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- `cargo test -p glass-browser --lib javascript_xhr_and_fragments_ignore_in_cell_end_tags --locked --quiet` — 1 passed; verifies all five ignored end-tag classes across direct document parsing, Rust fragment commit, same-turn projection, and XHR HTML parsing.
- `cargo test -p glass-browser --lib table --locked --quiet` — 45 passed.
- `python3 scripts/check-release-documentation.py --require-previous-version --report <temporary report>` — passed; 1,311 Markdown documents, 83 current documents, zero current-claim failures.
- `python3 scripts/check-documentation-depth.py` — passed; 93 current guides routed/audited and 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py` — passed; 15 implementation help keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py` — passed; 1,311 Markdown files, 346 MCP tools, 17 examples, and 22 public modules.
- `git diff --check` — passed after implementation and documentation updates.
- Remote CI, push, release, publication, and full browser conformance are not
  claimed by this local task.

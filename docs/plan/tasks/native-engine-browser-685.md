---
id: native-engine-browser-685
scope: native-engine/browser/html-table-mode-ignored-end-tags
status: done
depends-on: [native-engine-browser-684]
---

# Objective

Implement the WHATWG end-tag behavior for the "in table", "in caption", and
"in column group" insertion modes across all four native HTML parser routes.
In direct table mode, ignore `body`, `caption`, `col`, `colgroup`, `html`,
`tbody`, `td`, `tfoot`, `th`, `thead`, and `tr` before generic matching can
pop an ancestor; existing cell, row/row-group, and table-structure handlers
may continue to own their structural tokens. In caption mode, ignore
`body`, `col`, `colgroup`, `html`, `tbody`, `td`, `tfoot`, `th`, `thead`, and
`tr`, while allowing `</caption>` to close the active caption. In column-group
mode, ignore `</col>`, close `</colgroup>` only when the current node is that
group, and otherwise pop the active column group before reprocessing eligible
end tags in table mode.

The fragment context is not an open table, and foreign-current-node handling
remains on its existing route. Document parsing, Rust `innerHTML` commit,
same-turn JavaScript detached-fragment projection, and XHR HTML document
parsing must agree.

This is one bounded insertion-mode rule, not general HTML parser conformance.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-684.md`
- [WHATWG "in table" insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-intable)
- [WHATWG "in caption" insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incaption)
- [WHATWG "in column group" insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incolgroup)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-685.md`

## Verification

- `rustfmt --edition 2024 crates/glass-browser/src/browser/native_engine/dom.rs crates/glass-browser/src/browser/native_engine/javascript.rs` — passed.
- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- `cargo test -p glass-browser --lib javascript_xhr_and_fragments_follow_table_end_tag_modes --locked --quiet` — 1 passed; verifies direct-table, caption, and column-group end-tag parentage across document parsing, Rust fragment commit, same-turn projection, and XHR document parsing.
- `cargo test -p glass-browser --lib table --locked --quiet` — 46 passed.
- `python3 scripts/check-release-documentation.py --require-previous-version` — passed; 1,313 Markdown documents, 83 current documents, 63 previous-version hits, 1,425 semantic audit hits, and zero current-claim failures.
- `python3 scripts/check-documentation-depth.py` — passed; 93 current guides and 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py` — passed; 15 implementation help keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py` — passed; 1,313 Markdown files, 346 MCP tools, 17 examples, and 22 public modules.
- `rustfmt --check --edition 2024 crates/glass-browser/src/browser/native_engine/dom.rs crates/glass-browser/src/browser/native_engine/javascript.rs` and `git diff --check` — passed.
- Do not claim remote CI, push, release, publication, or full parser
  conformance from local verification.

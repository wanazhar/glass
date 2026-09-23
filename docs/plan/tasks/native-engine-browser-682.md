id: native-engine-browser-682
scope: native-engine/browser/html-table-in-cell-structural-start-recovery
status: done
depends-on: [native-engine-browser-681]
---

# Native Engine Browser Slice 682: In-Cell Structural Start Recovery

## Objective

When an HTML table-structure start tag is parsed with an open HTML `td` or
`th` in table scope, close that cell and its descendants, then process the
same token at the newly exposed table context. Cover the in-cell start-tag set:
`caption`, `col`, `colgroup`, `tbody`, `td`, `tfoot`, `th`, `thead`, and `tr`.

- A new `td` or `th` becomes a sibling in the active row.
- A new `tr` becomes a sibling row in the active row group.
- A caption, column, or row-group token unwinds the active row and row group
  so normal table construction places it under the containing table.
- Do not close an outer cell across nested HTML `table`, `template`, or `html`
  scope boundaries; a fragment context root is not an open cell.
- Preserve foreign-current-node handling and HTML namespace checks.

Align document parsing, Rust `innerHTML` commit, same-turn JavaScript fragment
projection, and XHR `responseType="document"` parsing. Test exact parentage
through nested cell descendants, each structural transition class, nested
table/template boundaries, foreign namespaces, and fragment contexts. This is
the structural-start portion of in-cell recovery, not a complete table
insertion-mode or active-formatting-list implementation.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- [WHATWG in-cell insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incell)
- [WHATWG in-row insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inrow)
- [WHATWG in-table-body insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-intablebody)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-682.md`

## Verification

- `rustfmt --edition 2024 crates/glass-browser/src/browser/native_engine/dom.rs crates/glass-browser/src/browser/native_engine/javascript.rs` — passed.
- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- `cargo test -p glass-browser --lib javascript_xhr_and_fragments_share_table_cell_structural_start_recovery --locked --quiet` — 1 passed.
- `cargo test -p glass-browser --lib table --locked --quiet` — 45 passed.
- The route fixture verifies exact parentage through document parsing, Rust `innerHTML` commit, same-turn JavaScript projection, and XHR HTML document parsing.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation.json` — 1,310 Markdown documents; zero current-claim failures.
- `python3 scripts/check-documentation-depth.py` — 93 current guides routed and audited.
- `python3 scripts/check-tui-shortcuts.py` and `python3 scripts/check-documentation-coverage.py` — passed; 1,310 Markdown files and all command/MCP/example/module inventories validated.
- `git diff --check` — passed.
- Remote CI, push, release, package publication, and full browser conformance are not claimed.

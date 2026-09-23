id: native-engine-browser-680
scope: native-engine/browser/html-table-row-group-end-tag-scope
status: done
depends-on: [native-engine-browser-679]
---

# Native Engine Browser Slice 680: HTML Table Row-Group and Row End-Tag Scope

## Objective

Apply HTML table-scope behavior to `</tbody>`, `</tfoot>`, `</thead>`, and
`</tr>` end tags across document parsing, Rust `innerHTML` commit, same-turn
JavaScript fragment projection, and XHR `responseType="document"` parsing:

- Close the matching open HTML row-group or row and its descendants.
- Consume an unmatched token in an HTML context instead of allowing the
  generic end-tag route to close an unrelated ancestor.
- Stop scope search at HTML `table`, `template`, or `html` boundaries.
- Do not count a fragment context element as an open element or let an
  identically named foreign-namespace element satisfy HTML scope.
- Keep a foreign-current-node token on the existing foreign/generic path.

Cover row-group and row closure, nested table/template boundaries, fragment
context, foreign namespace identity, exact resulting parentage, and all four
parser routes. This bounded stack rule does not claim a complete table
insertion-mode algorithm or full HTML parser conformance.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- [WHATWG in-table-body insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-intablebody)
- [WHATWG in-row insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inrow)
- [WHATWG in-cell insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incell)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-680.md`

## Verification

- `cargo check --quiet -p glass-browser --lib --tests --locked` — passed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked table` — 43 passed, 0 failed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked html_table_row_group_end_tags_stop_at_nested_table_scope` — 1 passed, 0 failed after adding the template-boundary fixture.
- `rustfmt --edition 2024 --check` on both touched Rust files — passed.
- Release documentation truth — 1,308 Markdown files, 83 current documents, 63 previous-version hits, 1,415 semantic audit hits, 0 current-claim failures.
- Documentation depth — 93 current guides, 19 substantive contracts; shortcut inventory — 15 implementation keys/63 documentation markers; coverage — 346 full-product tools, 101 browser-only tools, 17 examples, 22 public modules. All passed.
- `git diff --check` — passed.
- Remote CI, push, release, package publication, and full browser conformance are not claimed.

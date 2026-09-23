id: native-engine-browser-681
scope: native-engine/browser/html-table-cell-end-tag-scope
status: done
depends-on: [native-engine-browser-680]
---

# Native Engine Browser Slice 681: HTML Table Cell End-Tag Scope

## Objective

Apply HTML table-scope behavior to `</td>` and `</th>` end tags across
document parsing, Rust `innerHTML` commit, same-turn JavaScript fragment
projection, and XHR `responseType="document"` parsing:

- Close only the matching open HTML cell and its descendants.
- Consume an unmatched token in an HTML context instead of allowing the
  generic end-tag route to close an out-of-scope cell.
- Stop scope search at HTML `table`, `template`, or `html` boundaries.
- Do not count a fragment context element as an open element or let a
  same-named foreign-namespace element satisfy HTML scope.
- Keep a foreign-current-node token on the existing foreign/generic path.

Cover `td` and `th`, nested-table/template boundaries, fragment context,
foreign namespace identity, exact resulting parentage, and all four parser
routes. This is scoped stack closure only; it does not claim the complete
in-cell insertion mode, active-formatting-list maintenance, or general HTML
parser conformance.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- [WHATWG table scope](https://html.spec.whatwg.org/multipage/parsing.html#has-an-element-in-table-scope)
- [WHATWG in-cell insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incell)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-681.md`

## Verification

- `cargo check --quiet -p glass-browser --lib --tests --locked` — passed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib javascript_xhr_and_fragments_share_table_cell_end_tag_scope -- --nocapture` — 1 passed, 0 failed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked table` — 44 passed, 0 failed.
- `rustfmt --edition 2024 --check` on `dom.rs` and `javascript.rs` — passed.
- Release documentation truth — 1,309 Markdown files, 83 current documents, 63 previous-version hits, 1,417 semantic audit hits, 0 current-claim failures.
- Documentation depth — 93 current guides, 19 substantive contracts; shortcut inventory — 15 implementation keys/63 documentation markers; coverage — 346 full-product tools, 101 browser-only tools, 17 examples, 22 public modules. All passed.
- `git diff --check` — passed.
- Remote CI, push, release, package publication, and full browser conformance
  are not claimed.

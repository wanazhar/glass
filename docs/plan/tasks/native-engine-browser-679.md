id: native-engine-browser-679
scope: native-engine/browser/html-table-end-tag-scope
status: done
depends-on: [native-engine-browser-678]
---

# Native Engine Browser Slice 679: HTML Table End-Tag Scope

## Objective

Implement the bounded HTML tree-construction behavior for `</table>` end tags:

- In an HTML parsing context, search the open-element stack for an HTML
  `table` in table scope. HTML `template` and `html` elements are scope
  boundaries; the nearest matching table before a boundary is the target.
- When a target exists, pop it and all elements above it from the parser stack.
  When no target exists, consume and ignore the table end tag rather than
  closing an out-of-scope ancestor.
- An HTML fragment's context element is not itself an open element. A table
  context alone therefore does not make `</table>` effective; a parsed nested
  table can still be closed.
- Leave the existing foreign-current-node end-tag route unchanged.

Keep document parsing, Rust `innerHTML` commit, same-turn JavaScript fragment
projection, and XHR `responseType="document"` parsing coherent. Cover valid
nested-table closing, a table end tag crossing a template boundary, fragment
context handling, exact parentage, and foreign namespace behavior. This is one
scoped end-tag rule, not a complete HTML table insertion-mode state machine or
general parser conformance.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- [WHATWG table scope](https://html.spec.whatwg.org/multipage/parsing.html#has-an-element-in-table-scope)
- [WHATWG in-table insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-intable)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-679.md`

## Verification

- `cargo check --quiet -p glass-browser --lib --tests --locked` — passed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked table_end_tag` — 2 passed, 0 failed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked table` — 41 passed, 0 failed.
- `rustfmt --edition 2024 --check` on both touched Rust files — passed.
- Release documentation truth — 1,307 Markdown files, 83 current documents, 63 previous-version hits, 1,413 semantic audit hits, 0 current-claim failures.
- Documentation depth — 93 current guides, 19 substantive contracts; shortcut inventory — 15 implementation keys/63 documentation markers; coverage — 346 full-product tools, 101 browser-only tools, 17 examples, 22 public modules. All passed.
- `git diff --check` — passed.
- Remote CI, push, release, registry publication, and cross-platform certification are not claimed.

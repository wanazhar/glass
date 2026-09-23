---
id: native-engine-browser-676
scope: native-engine/browser/html-table-implied-containers
status: done
depends-on: [native-engine-browser-675]
---

# Native Engine Browser Slice 676: Implied Table Containers

## Objective

Implement the bounded HTML table insertion rule that synthesizes missing
section and row elements in every existing HTML parser route:

- A `tr` encountered with an HTML `table` as the current node gets an implicit
  `tbody` parent.
- A `td` or `th` encountered directly under an HTML `table` gets implicit
  `tbody` and `tr` parents.
- A `td` or `th` encountered directly under an HTML `tbody`, `thead`, or
  `tfoot` gets an implicit `tr` parent.

Apply the rule after implied end-tag/auto-close handling and before inserting
the authored token. Generated nodes use the normal namespace, node-count, and
DOM-depth rules. The rule must not activate for SVG/MathML table-like
elements. Keep the resulting tree coherent across document parsing, Rust
script `innerHTML` commits plus the same-turn JavaScript projection, and XHR
`responseType="document"` parsing.

Tests must assert exact `table > tbody > tr > (td|th)` parentage and authored
node order, cover explicit sections (which must not gain duplicates), cover
fragment contexts, and verify that foreign namespaces are unchanged. This is
one insertion-mode increment only; complete WHATWG table modes and general
HTML parser conformance remain open issue #40 gates.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- [WHATWG table insertion modes](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-intable)
- [WHATWG table-body insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-intablebody)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-676.md`

## Verification

- `cargo check --quiet -p glass-browser --lib --tests --locked`
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked table` — 36 passed, 0 failed.
- `cargo check --quiet -p glass-browser --lib --tests --locked` — passed.
- `rustfmt --edition 2024 --check` on changed Rust files, repository documentation validators, and `git diff --check` — passed.

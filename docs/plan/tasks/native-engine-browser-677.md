---
id: native-engine-browser-677
scope: native-engine/browser/html-table-implied-colgroup
status: done
depends-on: [native-engine-browser-676]
---

# Native Engine Browser Slice 677: Implied Column Group

## Objective

Extend the bounded table tree-construction behavior with the implied
`colgroup` rule in all current HTML parser routes. When a `col` start tag is
processed with an HTML `table` as the current node, synthesize an HTML
`colgroup` before inserting the authored column. Do not synthesize a duplicate
when the current node is already an explicit `colgroup`; do not activate the
rule for SVG/MathML table-like elements. Preserve authored order, namespace,
and the existing node/depth bounds.

Verify document parsing, Rust `innerHTML` commit plus same-turn JavaScript
projection, and XHR `responseType="document"`. Cover direct columns, explicit
column groups, and foreign namespaces. This is one table insertion-mode rule;
full WHATWG table behavior remains open issue #40 work.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- [WHATWG in-table insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-intable)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-677.md`

## Verification

- `cargo check --quiet -p glass-browser --lib --tests --locked` — passed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked colgroup` — 3 passed, 0 failed.
- `rustfmt --edition 2024 --check` on changed Rust files, repository documentation validators, and `git diff --check` — passed.

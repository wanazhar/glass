---
id: native-engine-browser-674
scope: native-engine/browser/html-fragment-parser
status: done
depends-on: [native-engine-browser-673]
---

# Native Engine Browser Slice 674: Fragment Table Foster Parenting

## Objective

Extend slice 673's bounded HTML table foster-parent insertion behavior to
script-driven fragment parsing in `NativeDocument::apply_script_inner_html` and
the shared JavaScript `populateDetachedFragment` same-turn projection.

For an HTML-namespace `table`, `tbody`, `tfoot`, `thead`, or `tr` current node,
foster decoded non-ASCII-whitespace text and ordinary start tags. Insert them
before the nearest parsed open HTML table in that table's actual parent. If the
fragment context itself is a table-structure element and no parsed table is
open below the context, use the fragment root as the insertion target so
fostered nodes remain inside the `innerHTML` target. Keep ASCII-whitespace-only
text, comments, and table-special tokens at their existing insertion location.
Assign namespaces from the actual insertion parent and push fostered
non-void elements onto the open-element stack so their descendants remain
inside them. SVG/MathML table-like elements do not activate HTML foster
parenting.

This is bounded parity for the existing fragment routes, not a complete HTML
fragment parser. The XHR `responseType="document"` parser, complete WHATWG
insertion modes, and general parser conformance remain separate issue #40 work.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [WHATWG HTML fragment parsing](https://html.spec.whatwg.org/multipage/parsing.html#html-fragment-parsing-algorithm)
- [WHATWG appropriate insertion location](https://html.spec.whatwg.org/multipage/parsing.html#appropriate-place-for-inserting-a-node)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-674.md`

## Verification

- `cargo check --quiet -p glass-browser --lib --tests --locked` — passed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked inner_html` — 5 passed, 0 failed.
- `rustfmt --edition 2024 --check crates/glass-browser/src/browser/native_engine/dom.rs crates/glass-browser/src/browser/native_engine/javascript.rs` — passed.
- Repository release-documentation, documentation-depth, documentation-coverage, and `git diff --check` validators — passed.

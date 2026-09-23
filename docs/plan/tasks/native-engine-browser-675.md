---
id: native-engine-browser-675
scope: native-engine/browser/html-response-document-parser
status: done
depends-on: [native-engine-browser-674]
---

# Native Engine Browser Slice 675: XHR Document Table Foster Parenting

## Objective

Extend the bounded table foster-parent behavior from slices 673–674 to the
separate JavaScript `nativeHtmlParseDocument` implementation used for XHR
`responseType="document"` with `text/html` responses.

For an HTML table-structure current node (`table`, `tbody`, `tfoot`, `thead`, or
`tr`), insert decoded non-ASCII-whitespace text and ordinary start tags before
the nearest open HTML table in its actual parent. Preserve ASCII-whitespace-only
text, comments, and table-special start tags at their existing insertion
location. Keep fostered non-void elements on the open-element stack so their
descendants remain attached. Preserve raw-node parent links, source order, node
limits, and the later implicit `html`/`head`/`body` normalization before DOM
materialization.

Use the same misnested-table fixture and observable element-order assertions as
the document parser and script `innerHTML` paths. This closes the bounded
three-route foster-parenting gap only; full WHATWG insertion modes, template
content, HTML parse-error recovery, and general parser conformance remain open
issue #40 work.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [WHATWG HTML tree construction](https://html.spec.whatwg.org/multipage/parsing.html#appropriate-place-for-inserting-a-node)

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-675.md`

## Verification

- `cargo check --quiet -p glass-browser --lib --tests --locked` — passed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked foster` — 8 passed, 0 failed.
- `rustfmt --edition 2024 --check` on changed Rust files — passed.
- Repository release-documentation, documentation-depth, documentation-coverage, and `git diff --check` validators — passed.

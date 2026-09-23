id: native-engine-browser-678
scope: native-engine/browser/html-nested-table-start-recovery
status: done
depends-on: [native-engine-browser-677]
---

# Native Engine Browser Slice 678: Nested Table Start Recovery

## Objective

Implement the bounded HTML tree-construction behavior for a `table` start tag
encountered while parsing in a table insertion mode:

- If an HTML `table` is open in table scope, pop through the active table and
  reprocess the incoming start tag against the remaining stack. Reprocessing
  may close additional active tables until the token is no longer in table
  mode.
- Do not apply that rule while the current open context is a cell or caption;
  valid nested tables in those contexts remain nested.
- In HTML fragment parsing, the context element is not itself an open table in
  the fragment stack. Ignore a `table` start token when the context establishes
  table insertion mode and the fragment stack contains no active table; allow
  table starts after parsing has entered a cell or caption context.
- Stop table-scope search at template boundaries. Do not trigger HTML rules
  when the current node is in a foreign namespace.

Keep document parsing, Rust `innerHTML` commit, same-turn JavaScript fragment
projection, and XHR `responseType="document"` behavior coherent. Cover exact
parentage/order, fragment-context behavior, nested cell/caption behavior,
template and foreign-namespace boundaries, and existing node/depth handling.
This is one table recovery rule only; general HTML parser conformance remains
open issue #40 work.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- [WHATWG in-table insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-intable)
- [WHATWG HTML fragment parsing](https://html.spec.whatwg.org/multipage/parsing.html#parsing-html-fragments)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-678.md`

## Verification

- `cargo check --quiet -p glass-browser --lib --tests --locked` — passed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked nested_table` — 4 passed, 0 failed.
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked table` — 39 passed, 0 failed.
- `rustfmt --edition 2024 --check` on both touched Rust files, repository documentation validators, and `git diff --check` — passed.
- Release documentation: 1,306 Markdown files, 83 current documents, 63 previous-version hits, 1,411 semantic audit hits, 0 current-claim failures. Depth: 93 guides/19 contracts. Coverage: 346 MCP tools, 17 examples, 22 public modules. TUI shortcuts: 15 implementation keys/63 documentation markers.
- Remote CI, push, release, registry publication, and cross-platform certification are not claimed.

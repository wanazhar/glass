---
id: native-engine-browser-713
scope: glass-browser/javascript/page-runtime-dynamic-imports
status: done
depends-on: [native-engine-browser-712]
---

# Objective

Support runtime-valued JavaScript `ImportCall`s in page classic and module
scripts without trying to predict their values during static module-graph
prefetch. Carry the active script's effective referrer into the existing
asynchronous content-process fetch path, then admit and evaluate the resolved
module through the bounded QuickJS module graph.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-712.md`
- [ECMAScript Import Calls](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html)
- [HTML Standard: module loading](https://html.spec.whatwg.org/multipage/webappapis.html)
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- A computed page `import(expression)` is issued only when the expression
  executes. It is not guessed or fetched by the static prefetch scanner.
- Classic scripts use their document or external response URL as referrer;
  module roots and dependencies use their host-tracked response base URL.
- The active referrer is used for Document import-map scope resolution. The
  resulting request travels through the existing resource-policy, CORS,
  integrity, redirect, byte, and graph-limit path before source admission.
- Nested computed imports from a fetched module re-enter the same asynchronous
  page request queue and resolve from that module's response base URL.
- Existing literal dynamic-import prefetch and static graph behavior remain
  unchanged. Worker computed imports, rooted-file computed imports, dynamic
  import options/attributes, and complete module task/TLA scheduling remain
  separate issue #40 work.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-713.md`

## Verification

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed.
- `cargo test -p glass-browser --lib --locked --quiet
  native_static_dynamic_import_tests` passed 4/4 tests.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_resolves_module_graphs_through_inline_import_maps`
  passed 1/1. It covers computed imports in an external classic script and an
  external module, import-map scope selection, a computed import nested in the
  fetched module, the expected HTTP request set, and clean module error state.
- `rustfmt --edition 2024` on the modified Rust sources and `git diff --check`
  passed.
- `cargo fmt --all -- --check` passed. Maintainer documentation gates passed:
  release truth validated 1,341 Markdown documents with zero current-claim
  failures; depth validated 93 current guides and 19 substantive contracts;
  shortcut inventory validated 15 implementation keys and 63 markers; coverage
  validated 1,341 Markdown files, 346 MCP tools (101 browser-only), 17
  examples, and 22 public modules.
- Local only; remote CI, release certification, cross-platform gates, and issue
  #40 closure are not claimed.

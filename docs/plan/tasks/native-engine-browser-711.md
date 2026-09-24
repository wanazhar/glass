---
id: native-engine-browser-711
scope: glass-browser/javascript/static-dynamic-import-specifiers
status: done
depends-on: [native-engine-browser-710]
---

# Objective

Extend module-graph prefetch to recognize dynamic `import()` first arguments
formed from fully static quoted string literals joined by `+`, including
parenthesized pieces and JavaScript whitespace/comments. Resolve a rooted-file
external module script's `src` as a URL relative to its document before
applying the configured file-root boundary; do not apply module-import bare
specifier rules to an HTML script URL. The shared discovery path must cover
HTTP(S) page graphs, dedicated/shared/service-worker module graphs, and
rooted-file module graphs without bypassing their existing URL, import-map,
integrity, origin, policy, byte, or graph-entry checks.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-710.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- Admit an argument only when the supported expression is entirely composed
  of quoted JavaScript string literals, `+`, balanced parentheses, and
  whitespace/comments. Do not partially prefetch expressions containing
  runtime values or other operators.
- Keep native QuickJS parsing/evaluation authoritative; this remains a bounded
  prefetch-discovery pass, not a JavaScript parser or runtime network loader.
- Keep request identity, final-response base, import-map resolution, fragment
  I/O handling, fetch policy, integrity, file-root validation, deduplication,
  and resource limits unchanged.
- Root module-script URL resolution accepts ordinary relative URLs such as
  `module.js`, while module dependency specifiers retain their existing
  import-map/bare-specifier policy.
- Test both recognized and rejected expressions and prove process-backed page,
  worker, and rooted-file module graphs fetch and evaluate concatenated
  dynamic-import dependencies.
- Runtime-valued computed imports and complete module scheduling remain
  separate issue #40 work; do not claim full dynamic-import conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-711.md`

## Verification

- Passed `cargo check -p glass-browser --lib --test native_engine --locked
  --quiet`.
- Passed the two focused `native_static_dynamic_import_tests` unit tests.
- Passed the process-backed static-expression page import test (1/1), the
  redirected worker module identity/base test with a computed dynamic import
  (1/1), and the rooted-file module/subresource test with a computed dynamic
  import and ordinary `src="module.js"` (1/1).
- Passed `cargo fmt --all -- --check` and `git diff --check`.
- Passed all four maintainer documentation gates: release truth covered 1,339
  Markdown files (83 current, 63 previous-version references, 1,482 semantic
  hits, zero current-claim failures); depth covered 93 guides and 19 contracts;
  shortcuts covered 15 implementation keys and 63 markers; coverage found 346
  MCP tools (101 browser-only), 17 examples, and 22 public modules.
- Remote CI was not run; this is local evidence only.

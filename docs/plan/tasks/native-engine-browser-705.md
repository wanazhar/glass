---
id: native-engine-browser-705
scope: glass-browser/native-engine/javascript/import-map-scopes
status: complete
depends-on: [native-engine-browser-704]
---

# Objective

Resolve page-module specifiers through bounded inline import-map `scopes` in
addition to the global `imports` map, using the same resolver for dependency
prefetch and QuickJS evaluation.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-704.md`
- [HTML Standard: import-map resolution and processing](https://html.spec.whatwg.org/multipage/webappapis.html#import-maps)

## Contract

- Parse scope URLs relative to the document URL and reject invalid or
  credential-bearing scope URLs.
- Parse each scope's bounded specifier map using the same exact and
  trailing-slash-prefix rules as global `imports`.
- For a module referrer, try matching scopes from most specific to least
  specific; within each map, prefer exact keys and then the longest prefix.
  If a matching scope has no entry for the specifier, continue through less
  specific scopes, then global imports. Unmapped bare specifiers still fail
  closed.
- Merge additional maps without replacing earlier global or scoped entries.
  Bound the combined number of mappings and scope URLs. Invalid input or a
  merge that would exceed a bound must not partially mutate the active map.
- Keep prefetch and QuickJS resolution on the same map and verify scoped
  resolution using a real local HTTP server and the native content process.
- This slice does not implement import-map `integrity`, external maps,
  post-load/dynamic registration, resolved-specifier locking, workers, or full
  parser/module scheduling. Do not claim complete import-map conformance.

## Paths

- `crates/glass-browser/src/browser/native_engine/module_import_map.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- this task record

## Verification

- Run the locked `glass-browser` library/test-target check before the focused
  import-map tests.
- Cover scope URL normalization, exact/prefix matching, most-specific scope,
  less-specific/global fallback, merge precedence, credentials, and combined
  resource bounds.
- A process-backed integration test must prove the selected scoped URLs are
  the actual requests and that evaluated modules use those fetched sources.
- Run formatting and the repository documentation gates affected by the new
  task and contract. Record remote CI separately; do not imply it ran locally.
- Passed `cargo check -p glass-browser --lib --test native_engine --locked
  --quiet`.
- Passed `cargo test -p glass-browser --lib --test native_engine import_map
  --locked --quiet`: 8 resolver/parser tests and 1 process-backed test,
  including nested scoped static/dynamic resolution and the exact HTTP paths.
- Passed `cargo fmt --all -- --check` and `git diff --check`.
- Documentation gates passed: release truth across 1,333 Markdown files (83
  current, 63 previous-version hits, 1,471 semantic hits, zero current-claim
  failures); depth across 93 guides/19 contracts; shortcut inventory across
  15 implementation keys/63 documentation markers; and coverage across 346
  full-product MCP tools, 17 examples, and 22 public modules.
- Remote CI was not run. The broader native-only, conformance, cross-platform,
  packaging, performance, and release gates remain open.

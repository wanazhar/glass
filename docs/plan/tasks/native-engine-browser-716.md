---
id: native-engine-browser-716
scope: glass-browser/javascript/classic-importscripts-runtime-dynamic-imports
status: done
depends-on: [native-engine-browser-715]
---

# Objective

Resolve runtime-valued JavaScript `ImportCall`s originating in statically
preloaded classic `importScripts()` dependencies of dedicated and shared
Workers. Each call must retain the final response URL of the classic script
where the expression executes, rather than inheriting the Worker entry URL.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-714.md`
- `docs/plan/tasks/native-engine-browser-715.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- Scope is the already-bounded classic script graph discovered through
  literal `importScripts()` URLs in dedicated and shared Workers.
- Rewrite only real runtime-valued `ImportCall`s per source file, carrying that
  resource's final response URL as referrer. Register every admitted referrer
  before the Worker graph executes. Redirected imported scripts use the final
  response URL, including its directory, for relative module resolution.
- Apply rewriting before classic-source concatenation, reserve the Worker
  entry referrer, retain distinct imported-source referrers only where calls
  were rewritten, and keep aggregate import/referrer accounting within the
  existing module-import bound.
- Preserve literal dynamic imports and `importScripts()` call counts/order.
  Worker resolution uses the existing worker module graph and never applies a
  Document import map.
- Reuse the bounded worker module loader, resource policy, response identity /
  base split, and graph entry/byte limits. Nested computed imports resolve from
  their own final response bases and settle in the owning Worker realm.
- Do not enable this rewrite in Service Worker classic imports until that
  owner has the same dynamic-module settlement path. Runtime-computed
  `importScripts()` arguments, rooted-file page imports, import options, and
  complete asynchronous module scheduling remain separate issue #40 work.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-716.md`

## Verification

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  import_scripts -- --nocapture` passed 3/3, including this task's local
  fixture and process-backed HTTP coverage plus the existing static
  `importScripts()` dependency regression.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_resolves_runtime_ -- --nocapture` passed 2/2 for
  existing dedicated and shared module-worker runtime imports.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_propagates_service_worker_document_csp_to_controlled_fetch
  -- --nocapture` passed 1/1 for the classic Service Worker integration path;
  `load_service_worker_source` keeps dynamic-import rewriting in `Preserve`
  mode for that owner.
- The process-backed HTTP fixture covers dedicated and shared classic Workers,
  redirected `importScripts()` source bases, nested computed module imports,
  exact request targets, and both Worker-to-page and SharedWorker port delivery.
- The local fixture regression covers the loader path and nested computed
  imports without HTTP transport.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Maintainer gates passed: release truth covered 1,344 Markdown files with 0
  current-claim failures; depth covered 93 guides/19 contracts; shortcut
  inventory covered 15 keys/63 markers; coverage validated 346 full-product
  MCP tools (101 browser-only), 17 examples, and 22 public modules.
- Local only. Remote CI and issue #40 closure are not claimed.

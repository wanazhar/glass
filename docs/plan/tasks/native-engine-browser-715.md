---
id: native-engine-browser-715
scope: glass-browser/javascript/shared-worker-runtime-dynamic-imports
status: done
depends-on: [native-engine-browser-714]
---

# Objective

Resolve runtime-valued JavaScript `ImportCall`s from classic and module
SharedWorker entry scripts. Load computed dependencies through the existing
bounded worker module path and settle success or failure in the SharedWorker
realm so connected page `MessagePort`s observe the result.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-714.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- Rewrite only real runtime-valued `ImportCall`s in classic and module
  SharedWorker entry scripts. Preserve the existing literal-import graph path.
- Validate classic imports against the active final response URL and module
  imports against the module response base. Resolve with worker URL/referrer
  rules; never apply a Document import map.
- Reuse the worker fetch path, resource policy, CORS, redirect handling,
  request-identity/response-base split, and graph entry/byte limits.
- Extend the owning SharedWorker module map before settling its import promise.
  Nested runtime-valued imports use their own active response bases.
- Resolve and reject through the existing SharedWorker realm and port routes;
  do not replace the SharedWorker runtime or collapse separate connections.
- Computed imports in classic `importScripts()` dependencies, rooted-file page
  imports, import options/attributes, and complete asynchronous module/task
  scheduling remain separate issue #40 work.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-715.md`

## Verification

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_resolves_runtime_ -- --nocapture` passed 2/2,
  covering dedicated-worker regression plus classic/module SharedWorker
  runtime imports, nested modules, rejection, exact HTTP requests, and
  connected-port delivery.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  shared_worker_reuses_named_runtime_and_ports` passed 2/2 for process-backed
  and local shared-runtime/port regressions.
- `cargo check -p glass-dev --bin glass --locked --quiet` and the scoped binary
  build passed to enable the maintainer inventory check.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Maintainer gates passed: release truth covered 1,343 Markdown files with 0
  current-claim failures; depth covered 93 guides/19 contracts; shortcut
  inventory covered 15 keys/63 markers; coverage validated 346 full-product
  MCP tools (101 browser-only), 17 examples, and 22 public modules.
- Local only. Remote CI and issue #40 closure are not claimed.

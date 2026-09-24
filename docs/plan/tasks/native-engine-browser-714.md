---
id: native-engine-browser-714
scope: glass-browser/javascript/dedicated-worker-runtime-dynamic-imports
status: done
depends-on: [native-engine-browser-713]
---

# Objective

Resolve runtime-valued JavaScript `ImportCall`s from dedicated classic and
module Worker entry scripts when invoked. Route requests through the existing
worker fetch owner, admit the bounded module graph, and settle the original
promise through the worker's QuickJS realm. Do not predict runtime values
during graph prefetch.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-713.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- Dedicated classic and module Worker entry scripts rewrite only real computed
  `ImportCall`s; literal imports retain their existing graph-discovery path.
- Each request carries its active Worker-script response URL as referrer. The
  host rejects inactive referrers and resolves against worker URL/referrer
  rules, without applying Document import maps.
- Module requests must use the credentialed, bodyless CORS `GET`/default-cache/
  follow-redirect shape. Load through the existing worker resource loader and
  graph byte/entry limits; preserve request identity and final-response bases.
- Static dependencies of a dynamically loaded module are admitted through the
  same bounded graph loader. Nested runtime-valued imports re-enter the worker
  fetch queue and resolve against their own response bases.
- Successful loads register a one-time internal module alias and settle through
  the owning Worker realm. Failed loads reject with the dynamic-import error.
- SharedWorker runtime imports, computed imports originating in classic
  `importScripts()` dependencies, rooted-file page imports, import
  options/attributes, and complete asynchronous module scheduling remain
  separate issue #40 work.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-714.md`

## Verification

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed.
- The process-backed `native_content_process_resolves_runtime_worker_module_imports`
  test passed 1/1 for dedicated classic/module Workers and a nested computed
  dependency.
- The existing process-backed
  `native_content_process_preserves_worker_module_identities_and_response_bases`
  regression passed 1/1 after enabling worker module rewriting.
- The scoped check and test output were quiet; the fixture asserted the exact
  six-request set, evaluated values, and an empty Worker error list.
- `cargo fmt --all -- --check` and `git diff --check` passed. Maintainer gates
  passed: release truth covered 1,342 Markdown documents with zero
  current-claim failures; depth covered 93 guides/19 contracts; shortcut
  inventory covered 15 keys/63 markers; coverage validated 346 MCP tools
  (101 browser-only), 17 examples, and 22 public modules.
- An exploratory SharedWorker probe fetched the computed dependencies but did
  not observe the resulting port messages. It is not counted as supported by
  this slice and remains a separate open behavior.
- Local only. Remote CI, SharedWorker computed-import settlement, rooted-file
  page computed imports, full profile conformance, and issue #40 closure are
  not claimed.

---
id: native-engine-browser-719
scope: glass-browser/javascript/service-worker-dynamic-imports
status: done
depends-on: [native-engine-browser-718]
---

# Glass native-engine browser slice 719: Service Worker dynamic-import rejection

## Objective

Make dynamic `import()` calls from every ServiceWorkerGlobalScope settle with
the HTML Standard's `TypeError` rejection, including calls in classic entry
scripts, classic scripts loaded through `importScripts()`, module entry
scripts, and their statically imported module dependencies.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-716.md`
- `docs/plan/tasks/native-engine-browser-718.md`
- [HTML Standard: HostLoadImportedModule](https://html.spec.whatwg.org/multipage/webappapis.html#hostloadimportedmodule)
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- A dynamic `import()` in a ServiceWorkerGlobalScope rejects its original
  promise with a `TypeError`; it must not fetch or evaluate the requested
  module. The same rejection applies to transitive dynamic imports.
- Apply the rule to the classic Service Worker entry and every classic source
  admitted through `importScripts()`, as well as module Service Worker entries
  and their statically imported module dependencies.
- Preserve argument evaluation and normal asynchronous promise rejection.
  Do not emit a worker `Fetch` command for a dynamic module request, including
  when the target is already present in a module graph. The rejection hook must
  be immutable to script code and retain its realm intrinsics so script
  mutation cannot bypass the no-fetch contract.
- Keep static `importScripts()` loading for classic Service Workers and static
  module imports for module Service Workers unchanged. Worker and SharedWorker
  dynamic-import support remains unchanged.
- Keep module graph traversal, source rewriting, referrers, and failure
  settlement bounded by the existing native script and module limits.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-719.md`

## Verification

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed. It initially reported the now-removed `Preserve` classic-loader
  variant as dead code; the final focused test builds passed after removal.
- `cargo test -p glass-browser --lib --test native_engine --locked --quiet
  service_worker_dynamic_import` passed 2/2 unit tests and 1/1 process-backed
  integration test. The test checks classic entry/importScripts calls, module
  entry/static-dependency calls, asynchronous `TypeError` rejection, evaluated
  specifier side effects, preserved static dependencies, attempted redefinition
  of the immutable rejection hook, and the exact request set. A module already
  in the static graph is rejected as a dynamic import; uncached dynamic
  targets are neither fetched nor prefetched.
- Existing worker regressions passed: `native_content_process_resolves_runtime_`
  passed 2/2 dedicated/shared module-worker tests, and
  `native_content_process_import_scripts_dynamic_imports_use_final_script_urls`
  passed 1/1.
- Final focused Service Worker regression passed 1/1 after adding the
  already-in-graph target and side-effect assertions.
- `rustfmt --edition 2024 --check` on the changed Rust files and `git diff
  --check` passed.
- Maintainer gates passed: release truth covered 1,347 Markdown files with
  zero current-claim failures; depth covered 93 guides/19 contracts; shortcut
  inventory covered 15 keys/63 markers; coverage validated 346 MCP tools (101
  browser-only), 17 examples, and 22 public modules.
- Local only. Remote CI and cross-platform certification were not run; issue
  #40 remains open.

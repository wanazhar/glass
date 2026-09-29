---
id: native-engine-browser-835
scope: glass-browser/native-engine/classic-worker-importscripts-referrer-policy
status: in-progress
depends-on: [native-engine-browser-834]
---

# Glass native-engine browser slice 835: classic Worker importScripts policy

## Objective

Load statically discovered classic `importScripts()` dependencies with the
correct Worker-global URL base, referrer source, and effective referrer policy
for DedicatedWorkers, SharedWorkers, and classic ServiceWorkers.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-834.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML Standard: worker processing model](https://html.spec.whatwg.org/multipage/workers.html#worker-processing-model)
- [HTML Standard: fetching an imported classic worker script](https://html.spec.whatwg.org/multipage/webappapis.html#fetch-a-classic-worker-imported-script)
- [HTML Standard: policy containers](https://html.spec.whatwg.org/multipage/browsers.html#policy-containers)
- [Fetch Standard](https://fetch.spec.whatwg.org/)
- [Referrer Policy: integration with Fetch](https://w3c.github.io/webappsec-referrer-policy/#integration-with-fetch)

## Contract

- Resolve every statically discovered classic `importScripts()` dependency,
  including nested dependencies, against the final URL of the root Worker
  script. A nested imported script's response URL must not become the base URL
  for another `importScripts()` call, because these calls use the owning
  WorkerGlobalScope's API base URL.
- Use the final root Worker URL as the referrer source for each dependency
  request, and use the effective policy container initialized for that Worker:
  recognized `Referrer-Policy` on a network root response, the standard
  `strict-origin-when-cross-origin` default when absent/unrecognized, or the
  creator Document policy for a supported `file:` Worker.
- A dependency response's `Referrer-Policy` does not replace the Worker-global
  policy for later `importScripts()` dependencies. Each dependency retains its
  own final response URL for the already separate dynamic `import()` rewriting
  path.
- Apply the same root URL and policy rule to classic ServiceWorker
  `importScripts()` dependency preloading, using the ServiceWorker root
  response's recognized policy or the standard default. This does not change
  ordinary ServiceWorker Fetch/XHR behavior.
- Preserve existing dependency graph limits, duplicate/cycle handling, CSP,
  redirects, resource loading, source concatenation, and Worker execution.
  Module Workers continue to reject `importScripts()`; module dependency and
  dynamic-import policy remain on their existing paths.
- A process-backed regression covers DedicatedWorker, SharedWorker, and
  ServiceWorker classic dependency graphs, nested root-based URL resolution,
  actual `Referer` headers under distinct root policies, and verifies that an
  imported script's response policy does not mutate the Worker-global policy.
  The test compiles here, but wire assertions require a host where its local
  HTTP listener can bind.
- This does not establish dynamic/runtime-valued `importScripts()` loading,
  complete Worker/ServiceWorker policy-container semantics, ordinary
  ServiceWorker Fetch/XHR inheritance, worklets, full Referrer Policy/Fetch or
  WPT conformance, remote CI, or cross-platform certification.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-835.md`

## Verification

- `cargo check -p glass-browser --features native-engine --lib --tests
  --locked -q` passed before the final removal of the now-unused loader
  adapter. The subsequent targeted Cargo test rebuilt the final package and
  integration-test source; compiler output contained only the existing
  superseded-HTML-parser dead-code warnings.
- `cargo test -p glass-browser --features native-engine --test native_engine
  --locked native_local_worker_import_scripts_nested_urls_use_worker_root_url
  -- --exact --quiet` passed (1 passed, 884 filtered; 10.85 seconds), verifying
  nested fixture `importScripts()` dependencies resolve against the root
  Worker URL.
- The process-backed DedicatedWorker/SharedWorker/ServiceWorker regression
  compiled into the integration-test executable, but was not run: prior
  Slice 833 execution established that this sandbox denies its first loopback
  listener bind with `PermissionDenied`. Actual HTTP `Referer` and Service
  Worker wire assertions remain unverified, so the slice stays in progress.
- Final release-truth audit passed: 1,463 Markdown files, 83 current
  documents, and zero current-claim failures. Documentation depth passed
  (93 current guides/19 substantive contracts); shortcut inventory passed
  (15 implementation keys/63 documentation markers); documentation coverage
  passed (346 full-product MCP tools, 101 browser-only, 17 examples, 22 public
  modules). `cargo fmt --all -- --check` and `git diff --check` also passed.
- Remote CI and issue #40 status were not updated in this environment.

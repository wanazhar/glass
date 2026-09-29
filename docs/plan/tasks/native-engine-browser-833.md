---
id: native-engine-browser-833
scope: glass-browser/native-engine/module-worker-referrer-policy
status: in-progress
depends-on: [native-engine-browser-832]
---

# Glass native-engine browser slice 833: module Worker referrer policy

## Objective

Carry the creator's effective referrer policy into DedicatedWorker and
SharedWorker module entry fetches, then preserve each module's effective
referrer policy and response URL through static and runtime `import()` graph
requests.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-832.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML Standard: worker processing model](https://html.spec.whatwg.org/multipage/workers.html#worker-processing-model)
- [HTML Standard: fetching scripts and module Worker graphs](https://html.spec.whatwg.org/multipage/webappapis.html#fetching-scripts)
- [HTML Standard: worker policy containers](https://html.spec.whatwg.org/multipage/browsers.html#policy-containers)
- [HTML Standard: `HostLoadImportedModule`](https://html.spec.whatwg.org/multipage/webappapis.html#hostloadimportedmodule-referrer-modulerequest-loadstate-payload)
- [Referrer Policy: redirect processing](https://w3c.github.io/webappsec-referrer-policy/#integration-with-fetch)

## Contract

- For Window-created DedicatedWorker and SharedWorker module scripts, carry the
  creator Document's current effective referrer policy into the initial worker
  script request. Its referrer source remains the creator Document URL. This
  includes the live Document policy after applicable meta updates; the Worker
  constructors do not gain a new `referrerPolicy` option.
- A module worker entry's recognized response `Referrer-Policy` header
  overrides the inherited policy for that module's dependencies. Each static
  dependency request uses the referencing module's final response URL as its
  referrer source. A dependency's recognized response policy becomes the
  policy for that module's own static and dynamic imports; missing or
  unrecognized values retain the inherited policy.
- Runtime `import()` from a module worker carries the active module's fetch
  policy and final response URL through the worker command bridge. The loaded
  module's response policy then flows through its static graph and subsequent
  runtime imports. Redirect response policy changes apply only to the next
  redirect hop; final response policy governs that module's descendants.
- Apply this to DedicatedWorker and SharedWorker module graphs without
  changing page `import()`, ordinary worker `fetch()`, XHR, classic
  `importScripts()`, or worker lifecycle/error behavior. Preserve module
  request identity/type, response URLs, credential modes, import resolution,
  CORS, CSP, integrity, cache/304 policy metadata, byte/redirect limits,
  deduplication, linking, and promise settlement.
- A process-backed HTTP regression verifies the creator-to-worker entry
  header, redirect policy update, module response override, static and
  runtime-dynamic child headers, and a nested import resolved from the final
  response URL. Pure tests verify policy metadata and transformed worker
  import commands.
- ServiceWorker module script graphs and their policy containers, policy
  defaults for ordinary requests initiated inside any WorkerGlobalScope,
  classic workers and `importScripts()`, nested Worker constructors,
  worklets, preload/modulepreload, full Referrer Policy/Fetch or WPT
  conformance, remote CI, and cross-platform certification remain separate
  requirements. This slice must not claim complete worker or browser
  conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-833.md`

## Verification

- `cargo check -p glass-browser --features native-engine --lib --tests
  --locked -q` passed.
- `cargo build -p glass-browser --features native-engine --bin
  glass-native-content-worker --locked -q` passed, ensuring the spawned child
  contains the current source.
- `cargo test -p glass-browser --features native-engine --lib --locked
  native_static_dynamic_import_tests --quiet` passed.
- `RUST_BACKTRACE=1 cargo test -q -p glass-browser --features native-engine
  --test native_engine --locked
  native_content_process_inherits_referrer_policy_through_module_workers --
  --exact --nocapture` passed (1 passed, 885 filtered; 18.84 seconds).
  The regression verifies live creator-meta policy on both worker entry
  redirects, redirect-policy updates on the final roots, response-policy
  inheritance for static and runtime-dynamic imports, no-referrer nested
  imports, and settled DedicatedWorker/SharedWorker messages. Module
  evaluation now remains pending while its dynamic module fetch is handled by
  the host, then records fulfillment or rejection through the worker event
  pump; the previous content-process exit no longer reproduces.
- The slice remains `in-progress` because Slice 832 is an explicit dependency
  and its inline page-module dynamic import still does not settle. Remote CI,
  WPT conformance, and cross-platform certification remain unclaimed.
- Documentation truth passed (1,464 Markdown documents, 83 current documents,
  zero current-claim failures); depth passed (93 routed guides, 19 substantive
  contracts); coverage passed (1,464 Markdown files, 346 MCP tools, 17
  examples, 22 public modules). `rustfmt --edition 2024` and `git diff --check`
  passed for this checkpoint.
- Keep this slice `in-progress` until the process-backed assertions run in an
  environment that permits loopback listeners. Remote CI, WPT conformance,
  and cross-platform certification remain unclaimed.

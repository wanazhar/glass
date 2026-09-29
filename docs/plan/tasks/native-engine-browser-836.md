---
id: native-engine-browser-836
scope: glass-browser/native-engine/service-worker-referrer-policy-container
status: in-progress
depends-on: [native-engine-browser-835]
---

# Glass native-engine browser slice 836: ServiceWorker referrer policy container

## Objective

Carry the ServiceWorker script resource's effective referrer policy through
module dependency loading and into Worker-global Fetch and asynchronous XHR,
preserving request-level policy and public Request state.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-835.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Service Workers: script resource policy container and worker startup](https://w3c.github.io/ServiceWorker/#run-a-service-worker)
- [HTML Standard: worker policy containers](https://html.spec.whatwg.org/multipage/browsers.html#policy-containers)
- [HTML Standard: worker processing model](https://html.spec.whatwg.org/multipage/workers.html#worker-processing-model)
- [Fetch Standard: main fetch](https://fetch.spec.whatwg.org/#main-fetch)
- [Referrer Policy: integration with Fetch](https://w3c.github.io/webappsec-referrer-policy/#integration-with-fetch)

## Contract

- Initialize the ServiceWorker global's effective referrer policy from the
  fetched root script response's recognized `Referrer-Policy` value, falling
  back to `strict-origin-when-cross-origin` when no recognized response token
  exists. The ServiceWorker policy container comes from its script resource;
  it is not inherited from the registering Document's policy.
- Use the final ServiceWorker script URL as the global Fetch/XHR referrer
  source. A Fetch request with an empty request-level referrer policy uses the
  ServiceWorker-global policy at dispatch. Explicit `RequestInit` and source
  `Request` policies win, and an empty public `Request.referrerPolicy` remains
  empty when it inherits the global default.
- Asynchronous XHR initiated by the ServiceWorker uses the same global
  default. Do not add synchronous XHR support: synchronous requests are not
  permitted in a ServiceWorker.
- For module ServiceWorkers, the root script's effective response policy is
  inherited by its static module dependencies; a recognized dependency
  response policy becomes the policy for that module's own dependencies.
  Preserve the existing module URL/referrer, CORS, redirect, and graph rules.
- Preserve FetchEvent request policy/provenance when script code forwards or
  clones `event.request`; do not replace it with the ServiceWorker-global
  default. Classic `importScripts()` policy is handled by Slice 835, and the
  ServiceWorker registration entry request remains separate.
- A process-backed regression covers actual same-origin `Referer` headers for
  root-response policies (including missing/unrecognized fallback), explicit
  Fetch override, empty public Request policy, asynchronous XHR, and module
  dependency inheritance. Its HTTP assertions require a host where the local
  listener can bind. Pure tests verify the service-worker bootstrap command
  policy without process-backed networking.
- This does not establish registration-entry request policy, synchronous XHR,
  dynamic ServiceWorker module imports, navigation-preload policy,
  background/sync service-worker APIs, complete policy-container persistence,
  full Referrer Policy/Fetch or WPT conformance, remote CI, or cross-platform
  certification.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-836.md`

## Verification

- `cargo check -p glass-browser --features native-engine --lib --tests
  --locked -q` passed after resolving the duplicate shared-worker bootstrap
  arguments. Output contained only the existing dead-code warnings from the
  superseded HTML parser.
- `cargo test -p glass-browser --lib --features native-engine --locked
  service_worker_fetch_and_async_xhr_use_global_policy_without_changing_request_policy
  -- --quiet` passed (1 passed, 1,650 filtered; 0.16 seconds), verifying
  inherited/explicit Fetch policy commands, async-XHR default policy, and
  unchanged public Request policy values.
- `native_content_process_service_worker_fetches_use_script_policy_container`
  passed its focused process-backed HTTP regression locally. It verifies the
  ServiceWorker policy container on module dependencies, Fetch, and async XHR.
  Slice 836 remains `in-progress` because its declared dependency chain still
  includes Slice 833, whose process-backed regression now passes but remains
  `in-progress` pending Slice 832.
- Final release-truth audit passed: 1,464 Markdown files, 83 current
  documents, and zero current-claim failures. Documentation depth passed
  (93 current guides/19 substantive contracts); shortcut inventory passed
  (15 implementation keys/63 documentation markers); documentation coverage
  passed (346 full-product MCP tools, 101 browser-only, 17 examples, 22 public
  modules). `cargo fmt --all -- --check` and `git diff --check` also passed.
- Remote CI and issue #40 status were not updated in this environment.

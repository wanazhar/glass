---
id: native-engine-browser-834
scope: glass-browser/native-engine/worker-global-referrer-policy
status: in-progress
depends-on: [native-engine-browser-833]
---

# Glass native-engine browser slice 834: Worker-global referrer policy

## Objective

Use the initialized DedicatedWorker and SharedWorker policy container as the
effective default for ordinary Worker Fetch and XHR requests, without changing
the public `Request.referrerPolicy` value or explicit request overrides.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-833.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML Standard: worker processing model](https://html.spec.whatwg.org/multipage/workers.html#worker-processing-model)
- [HTML Standard: worker policy containers](https://html.spec.whatwg.org/multipage/browsers.html#policy-containers)
- [Fetch Standard](https://fetch.spec.whatwg.org/)
- [Referrer Policy: integration with Fetch](https://w3c.github.io/webappsec-referrer-policy/#integration-with-fetch)

## Contract

- For network-backed DedicatedWorker and SharedWorker scripts, initialize the
  worker-global effective referrer policy from the final top-level script
  response's recognized `Referrer-Policy` value. If the response has no
  recognized value, use `strict-origin-when-cross-origin`, the policy
  container default. Do not substitute the creator Document's policy here;
  that policy remains relevant to the initial Worker script fetch and is
  already covered by Slice 833 for module entry requests.
- For `file:` Workers, inherit the creator's current Document policy, matching
  the local-worker policy-container clone. Other local/blob URL policy
  container provenance remains outside this bounded slice.
- An ordinary worker `fetch()` with an empty request policy uses the effective
  worker-global policy at dispatch. An explicit supported `RequestInit` or
  source-`Request` policy wins. Keep the observable `Request.referrerPolicy`
  empty when the request inherits its worker-global default.
- Asynchronous and synchronous Worker XHR use the same effective
  worker-global default. Page XHR behavior is unchanged. DedicatedWorker and
  SharedWorker, classic and module worker roots, share the same initialization
  rule.
- A process-backed HTTP regression checks response-header policy, absent and
  unrecognized-header defaults, explicit Fetch override, public Request
  property preservation, asynchronous XHR, synchronous XHR, and SharedWorker
  routing through actual request `Referer` headers. Pure tests cover policy
  container selection and Fetch command propagation.
- ServiceWorker policy containers and requests, `importScripts()`, nested
  Worker creation, module dependency/import policy, worklets, non-file local
  schemes, full Referrer Policy/Fetch or WPT conformance, remote CI, and
  cross-platform certification remain separate requirements. This slice must
  not claim complete worker or browser conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-834.md`

## Verification

- `cargo check -p glass-browser --features native-engine --lib --tests
  --locked -q` passed, with only the existing dead-code warnings from the
  superseded HTML parser.
- `cargo test -p glass-browser --features native-engine --lib --locked
  native_static_dynamic_import_tests --quiet` passed (11 passed, 1,639
  filtered; 0.15 seconds). This includes network/local policy-container
  selection and Worker Fetch command/public-property assertions.
- `native_content_process_worker_fetch_and_xhr_use_worker_policy_container`
  passed in the focused process-backed regression batch. Its Worker Fetch and
  XHR wire assertions ran locally. Slice 834 remains `in-progress` because its
  dependency chain includes Slice 833, whose process-backed regression now
  passes but remains `in-progress` pending Slice 832.
- Documentation truth passed for 1,462 Markdown files with zero current-claim
  failures; depth, shortcut inventory, coverage, formatting, and whitespace
  checks also passed. Remote CI, WPT conformance, and cross-platform
  certification were not run or claimed.

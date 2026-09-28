---
id: native-engine-browser-814
scope: glass-browser/shared-worker-module-credentials
status: planned
depends-on: [native-engine-browser-813]
---

# Glass native-engine browser slice 814: SharedWorker module credentials

## Objective

Apply `SharedWorkerOptions.credentials` to module SharedWorker script fetches
without changing classic-worker fetching. Preserve the selected credentials
mode through the initial module script and its fetched module graph, including
redirects and response-cookie handling.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) owns native-browser
  completion and closure gates.
- The versioned [Glass Core Web Profile](../native-engine-browser-profile.md)
  defines required worker/network behavior.
- [Slice 813](native-engine-browser-813.md) establishes constructor matching
  and rejects reuse when credentials differ.
- The [HTML Standard worker processing model](https://html.spec.whatwg.org/multipage/workers.html#run-a-worker)
  passes the credentials option to module worker script-graph fetching, while
  classic worker fetching does not consume that option. The
  [Fetch Standard credentials modes](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)
  define `omit`, `same-origin`, and `include` request/response behavior.

## Contract

- For a module SharedWorker, apply its validated credentials mode to the root
  script and module graph requests; the same mode governs credential sending
  and whether response credentials are accepted.
- Do not apply the SharedWorker `credentials` option to classic-script fetches.
  The option still participates in constructor matching as specified by Slice
  813.
- Preserve existing URL parsing, CSP, mixed-content, CORS, redirect limits,
  response bounds, cookie policy, and error delivery. Fail closed when the
  request cannot satisfy the existing security policy.
- Do not broaden this slice to dedicated Workers, unrelated Fetch/XHR paths,
  agent-cluster policy, worker lifetime timers, or complete WPT conformance.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-814.md`

## Verification

- Run `cargo check -p glass-browser --test native_engine --locked --quiet`
  before the focused behavior test.
- Add real-HTTP process-backed tests that distinguish the three credentials
  modes through observed request cookies and accepted response cookies, cover
  module-root/static-dependency and redirect requests, and show classic-worker
  script fetching is unaffected by the option.
- Verify cross-origin module responses still obey CORS and that error paths do
  not create or poison a SharedWorker registry entry.
- Run `cargo fmt --all -- --check`, `git diff --check`, and the four maintainer
  documentation gates; record exact output, warnings, elapsed time, and
  remaining boundaries.
- Keep the checkpoint local; do not push or run remote CI.

## Results

Planned. No implementation or validation is claimed yet.

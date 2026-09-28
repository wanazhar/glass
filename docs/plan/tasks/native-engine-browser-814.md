---
id: native-engine-browser-814
scope: glass-browser/shared-worker-module-credentials
status: done
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
  script, static module graph, and dynamic imports issued from that worker;
  the same mode governs credential sending and whether response credentials
  are accepted.
- Carry the creating content process's bounded cookie profile with the
  SharedWorker-create effect into the session-level worker loader, so the
  credentials mode is evaluated against the actual browser-session cookie
  state rather than the coordinator's construction-time snapshot.
- Do not apply the SharedWorker `credentials` option to classic-script fetches.
  The option still participates in constructor matching as specified by Slice
  813.
- Preserve existing URL parsing, CSP, mixed-content, CORS, redirect limits,
  response bounds, cookie policy, and error delivery. Fail closed when the
  request cannot satisfy the existing security policy. Process redirect
  response cookies before following the next URL, subject to the request's
  credentials mode.
- Do not reuse a credentialed module response from a cache entry fetched under
  a different credentials mode; if the existing bounded cache cannot express
  the mode safely, bypass it for these worker module requests.
- Do not broaden this slice to dedicated Workers, unrelated Fetch/XHR paths,
  agent-cluster policy, worker lifetime timers, or complete WPT conformance.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
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
  module-root/static-dependency, dynamic-import, and redirect requests, and
  show classic-worker script fetching is unaffected by the option.
- Verify cross-origin module responses still obey CORS and that a failed load
  does not poison a SharedWorker identity or prevent a later retry.
- Run `cargo fmt --all -- --check`, `git diff --check`, and the four maintainer
  documentation gates; record exact output, warnings, elapsed time, and
  remaining boundaries.
- Keep the checkpoint local; do not push or run remote CI.

## Results

Implemented and validated locally.

- `cargo fmt --all -- --check` and `git diff --check` passed after
  implementation.
- `cargo check -p glass-browser --test native_engine --locked --quiet` passed
  in about 51 seconds. It reported existing dead-code warnings for the
  superseded legacy HTML parser in `native_engine/dom.rs`; no new warning or
  error was identified.
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_module_credentials_cover_redirects_and_graph_cookies --locked --quiet -- --exact`
  passed: 1 passed, 865 filtered, 27.07 seconds for the test.
- The process-backed HTTP regression verifies content-process cookie-profile
  handoff to the session loader; `omit`, `same-origin`, and `include` on
  same-origin redirected module roots; immediate redirect `Set-Cookie`
  processing; all three modes across cross-origin static dependencies and a
  dynamic import; CORS rejection followed by a successful retry using the
  same worker identity; and classic SharedWorker fetch behavior remaining
  unaffected by `credentials: "omit"`.
- Credential-mode module requests bypass the existing script cache because
  its cache key does not encode credentials mode. The response-cookie state
  is exercised within the session-level worker loader; broader cross-context
  cookie-store synchronization/persistence is not claimed by this slice.
- All four maintainer documentation gates passed. Their reported summaries
  were: release truth, `1442 Markdown documents; current documents=83;
  previous-version hits=63; semantic audit hits=1617; current-claim
  failures=0`; depth, `93 current guides routed/audited, 19 substantive
  contracts`; shortcut inventory, `15 implementation help keys; 63
  documentation markers`; coverage, `1442 Markdown files, 346 full-product
  MCP tools (101 browser-only), 17 examples, 22 public modules`. Measured
  command wall times were 1.97s, 0.17s, 0.06s, and 2.47s respectively.
  `cargo fmt --all -- --check && git diff --check` took 9.50s.
- This is targeted local coverage, not a full Web Platform Test run,
  cross-platform validation, or remote CI result. Issue #40 remains open for
  the remaining browser requirements.

---
id: native-engine-browser-809
scope: glass-browser/shared-worker-cross-target-runtime-routing
status: done
depends-on: [native-engine-browser-808]
---

# Glass native-engine browser slice 809: SharedWorker reuse across page targets

## Objective

Verify that two same-origin top-level page targets in one native browser
session connect to the same SharedWorker runtime and exchange messages through
their independent connection ports.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard's shared-worker matching algorithm](https://html.spec.whatwg.org/multipage/workers.html#shared-workers)
  matches a live worker by constructor storage key, resolved constructor URL,
  and name. A shared worker is owned by the user agent's shared-worker manager.
- `NativeEngineBackend` creates a `NativeEngine` for each active or parked
  page target. Each engine currently owns a `NativeWorkerRegistry`; same-page
  connections therefore do not prove the shared-worker manager spans targets.
- Slice [808](native-engine-browser-808.md) verifies bridge close isolation
  among three connections within one page. It explicitly leaves cross-page
  SharedWorker reuse unverified.

## Contract

- Use one `BrowserRuntimeSession` and two top-level targets loaded over local
  HTTP from the same origin. Both pages must construct a SharedWorker with the
  same resolved script URL, name, and classic-worker options.
- Prove that both page ports reach one worker global. The worker reports a
  monotonically increasing connection number and stable runtime identity; the
  two connections must observe the same runtime and distinct connection
  numbers rather than two separately initialized workers.
- Send a request from each page connection and verify that the shared worker
  delivers the corresponding response to the other page's port. Target
  selection must not replace or reset the worker global or either page's
  connection state.
- Use a deterministic process-backed `NativeEngineBackend`/HTTP regression.
  Do not infer cross-target behavior from multiple `SharedWorker` constructors
  in one Document or from separately created `NativeEngine` instances.
- If the regression exposes per-target ownership as the defect, add only the
  bounded session-owned SharedWorker coordination needed to share the matching
  worker global and route each connection. Keep Dedicated Worker and Service
  Worker ownership separate. Preserve bounded queues, target identity, and
  route-specific failure/close behavior.
- Do not broaden this slice into multiple browser sessions, cross-process
  sharing, storage-partition policy beyond the same-session match, owner
  destruction/GC lifetime, BFCache, option-mismatch errors, or full
  SharedWorker/EventTarget/WPT conformance.

## Boundaries and tradeoffs

- Two same-origin targets in one native session exercise the missing
  integration seam between target-owned pages and a user-agent-owned shared
  worker manager. The test proves only this session scope; it does not certify
  sharing between independent sessions or processes.
- Requiring cross-target request/reply makes both runtime identity and actual
  port routing observable. It does not claim complete SharedWorker lifetime,
  scheduling, storage-key, or worker-options conformance.

## Paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`

## Verification

Run the scoped check before the exact process-backed regression:

```console
cargo check -p glass-browser --test native_engine --locked --quiet
cargo test -p glass-browser --test native_engine native_runtime_shared_worker_reuses_runtime_across_targets --locked --quiet -- --exact
```

The initial regression failed as expected: the second same-origin target
reported connection 1 instead of connection 2. The cause was one independent
content-process worker registry per target. The backend now owns one bounded
SharedWorker registry per `BrowserRuntimeSession`; process-backed pages forward
only SharedWorker creation to it, while page MessagePort traffic is routed by
its existing context/frame owner. Dedicated Worker and Service Worker
ownership are unchanged. Initial page effects now survive document commit,
and worker-to-page replies are distinguished from page-to-worker commands.

Evidence after the fix:

- `cargo check -p glass-browser --test native_engine --locked --quiet` passed;
  only existing HTML-parser dead-code warnings were emitted.
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_reuses_runtime_across_targets --locked --quiet -- --exact` passed (1 passed, 861 filtered; 36.22 seconds).
- The process-backed test verifies one runtime identity, connection numbers 1
  and 2, and successful page-to-worker-to-page relays in both directions after
  selecting the other target.
- `cargo fmt --all`, `git diff --check`, and all maintainer handbook
  documentation gates passed. The Markdown truth audit covered 1,437 files
  with zero current-claim failures; depth validated 93 guides and 19
  substantive contracts; shortcut inventory validated 15 implementation
  keys and 63 markers; coverage validated 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules.

This does not certify cross-session/process sharing, worker destruction and
last-client lifetime, complete storage-key/agent-cluster matching, task-source
scheduling, option-mismatch behavior, WPT, remote CI, or cross-platform
conformance. Those remain issue #40 gates.

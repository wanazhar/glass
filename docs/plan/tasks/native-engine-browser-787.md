---
id: native-engine-browser-787
scope: glass-browser/classic-dedicated-worker-startup-runtime-errors
status: completed
depends-on: [native-engine-browser-786]
---

# Glass native-engine browser slice 787: classic dedicated-worker startup errors

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML WorkerGlobalScope contract](https://html.spec.whatwg.org/multipage/workers.html#workerglobalscope-and-the-workerglobalscope-mixin)
  exposes `onerror` as an `OnErrorEventHandler`.
- The [HTML exception-reporting algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#report-an-exception)
  first fires a cancelable `ErrorEvent` at the worker global. Its special
  handler is called with message, filename, line, column, and error; exact
  `true` cancels. If unhandled, a dedicated-worker error is queued to its
  owning `Worker`.
- On the owning `Worker`, the error handler receives the `ErrorEvent` object;
  a false return follows ordinary EventHandler cancellation, and later
  listeners still run.

## Objective

Report an uncaught runtime exception from a classic dedicated worker's initial
script inside the worker realm before forwarding an uncanceled error to the
page's `Worker`. Preserve the worker after the failed script so later tasks
can still be processed.

## Contract

- A thrown value from the initial classic dedicated-worker script creates a
  cancelable worker-realm `ErrorEvent` with worker-global target/currentTarget,
  type `error`, and the available message and worker URL. The bounded engine
  currently reports line and column as zero.
- `WorkerGlobalScope.onerror` is invoked with the legacy five arguments. Only
  an exact `true` return cancels forwarding; `false`, `0`, and other values do
  not. Registered `error` listeners still run and observe the event's final
  `defaultPrevented` state.
- If the worker-global event is not canceled, the owning page `Worker` receives
  an `ErrorEvent` whose target/currentTarget are that `Worker`. Its ordinary
  event-handler return semantics apply, and later listeners still run.
- The original exception object stays in the worker realm; the forwarded
  event's `error` is null. A canceled or uncanceled startup exception does not
  destroy the worker; its already-installed handlers can process a later
  message.
- Only uncaught runtime exceptions from the initial classic dedicated-worker
  script take this path. Existing worker resource/parse failure handling is
  unchanged.

## Boundaries and tradeoffs

- This does not implement worker module startup errors, load/CSP/parse failure
  `ErrorEvent` details, exceptions thrown by later worker event callbacks,
  promise rejection reporting, shared-worker error propagation, nested-worker
  chains, precise source locations, console reporting, or full worker/WPT
  conformance.
- The compatibility benefit is that classic worker code can observe and
  suppress its own uncaught startup error without silently losing the worker.
  QuickJS currently supplies bounded message text but not interoperable
  line/column/source-map metadata; those values stay zero rather than being
  guessed.
- Issue #40's native-only production, remote CI, cross-platform, and release
  gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-787.md`

## Verification

Passed locally:

- `rustfmt --edition 2024 --check crates/glass-browser/src/browser/native_engine/javascript.rs crates/glass-browser/src/browser/native_engine/content_process.rs crates/glass-browser/src/browser/native_engine/service_worker.rs crates/glass-browser/tests/native_engine.rs`
- `cargo check -p glass-browser --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked --quiet -- --exact native_local_dedicated_worker_startup_errors_report_and_forward --test-threads=1`
  (1 passed; verifies exact-true worker-global cancellation, uncanceled owner
  forwarding and event order, later worker-message handling, and distinct
  syntax-error reporting)
- `python3 scripts/check-release-documentation.py --require-previous-version`
  (1,415 Markdown documents; 0 current-claim failures)
- `python3 scripts/check-documentation-depth.py`
  (93 current guides routed/audited; 19 substantive contracts)
- `python3 scripts/check-tui-shortcuts.py`
  (15 implementation help keys; 63 documentation markers)
- `python3 scripts/check-documentation-coverage.py`
  (1,415 Markdown files; 346 MCP tools, 17 examples, 22 public modules)
- `git diff --check`

The scoped Cargo check/test pass with existing dead-code warnings in the
native DOM module. Remote CI, full worker/WPT conformance, cross-platform
certification, and issue #40 native-only production gates remain open.

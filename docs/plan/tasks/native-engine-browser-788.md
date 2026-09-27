---
id: native-engine-browser-788
scope: glass-browser/module-dedicated-worker-startup-runtime-errors
status: in_progress
depends-on: [native-engine-browser-787]
---

# Glass native-engine browser slice 788: dedicated module-worker startup errors

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML module-script processing model](https://html.spec.whatwg.org/multipage/webappapis.html#calling-scripts)
  reports a rejected module evaluation for the module script's settings global.
- The [HTML worker runtime-error rule](https://html.spec.whatwg.org/multipage/workers.html#runtime-script-errors)
  reports an uncaught runtime exception for its `WorkerGlobalScope`.
- Slice [787](native-engine-browser-787.md) defines the worker-global
  `ErrorEvent`, legacy `onerror`, exact-`true` cancellation, owner forwarding,
  and worker-survival behavior for classic dedicated-worker startup.

## Objective

Apply the same worker-global runtime-error reporting to a rejected evaluation
of the initial dedicated module-worker script, while preserving the distinct
owner-side error path for failures before module evaluation and keeping the
worker available after the exception.

## Contract

- When the initial dedicated module worker has loaded, parsed, and linked, and
  its module evaluation rejects during the current synchronous startup turn,
  report the rejection in the worker realm as a cancelable `ErrorEvent`.
- Reuse slice 787's worker-global handler order and shape: invoke
  `WorkerGlobalScope.onerror` with the legacy five arguments, cancel forwarding
  only for exact `true`, then run registered `error` listeners so they observe
  the final `defaultPrevented` state.
- If uncanceled, forward an `ErrorEvent` to the owning page `Worker`; keep the
  exception object in the worker realm, set the forwarded event's `error` to
  null, and preserve owner-handler cancellation and later-listener order.
- The worker remains able to process a later message after either a handled or
  unhandled startup evaluation exception.
- Module fetch, parse, resolution, and link failures remain startup failures
  reported through the existing owner-side path; they are not reclassified as
  runtime exceptions inside the worker global.
- Do not treat a pending top-level-await evaluation as an immediate runtime
  error. Eventual top-level-await settlement/rejection is outside this slice.
- Limit the behavior change to the initial dedicated module worker. Later
  module-worker turns, shared/module service workers, and their event tasks are
  unchanged.

## Boundaries and tradeoffs

- This closes the missing initial module-evaluation runtime-error counterpart
  to slice 787 without changing module graph loading or the worker task model.
- QuickJS does not provide interoperable source positions through the current
  bounded error bridge; line and column remain zero. Full module top-level
  await scheduling, later callback and promise-rejection reporting, shared and
  service-worker error propagation, CSP paths, and worker/WPT conformance
  remain open.
- Issue #40's native-only production, remote CI, cross-platform, and release
  gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-788.md`

## Verification

Pending implementation and focused verification.

---
id: native-engine-browser-786
scope: glass-browser/shared-worker-connect-message-event
status: completed
depends-on: [native-engine-browser-785]
---

# Glass native-engine browser slice 786: shared-worker connect `MessageEvent`

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML SharedWorkerGlobalScope contract](https://html.spec.whatwg.org/multipage/workers.html#shared-workers-and-the-sharedworkerglobalscope-interface)
  exposes `onconnect` as an `EventHandler`.
- The [HTML shared-worker connection algorithm](https://html.spec.whatwg.org/multipage/workers.html)
  fires `connect` using `MessageEvent`, with empty `data`, a frozen `ports`
  array containing the inside port, and `source` set to that port.
- Ordinary HTML EventHandler processing cancels on an exact `false` return;
  ordinary `addEventListener` callback return values are ignored.

## Objective

Replace the shared worker's hand-built `connect` event object with the
worker-realm `MessageEvent` contract and honor `onconnect` exact-false
cancellation before later listeners, without changing connection scheduling
or transferred-port ownership.

## Contract

- The connect event is a worker-realm `MessageEvent` with type `connect`, empty
  `data`, the incoming inside port as `source`, and a frozen `ports` array
  containing that same port.
- The event is non-bubbling and non-cancelable. During dispatch, `target` and
  `currentTarget` are the shared worker global; `currentTarget` and phase are
  cleared after dispatch.
- The `onconnect` handler runs before registered listeners. An exact `false`
  return sets `defaultPrevented` despite `cancelable` being false. Other return
  values do not cancel. Later listeners still run and see the handler's
  canceled state; listener callback returns remain ignored.
- Existing shared-worker port ownership, task ordering, and native queue
  behavior remain unchanged.
- The focused integration test uses two connections: the first handler returns
  `false`, the second returns `0`, and each port reports the event shape and
  cancellation state observed by a later listener.

## Boundaries and tradeoffs

- This does not implement broader shared-worker lifetime/garbage-collection
  semantics, connect-task source arbitration, full worker EventTarget listener
  options/propagation, worker CSP/error reporting, callback exception
  reporting, specialized XHR/WebSocket/EventSource dispatchers, or full worker
  and Web Platform Test conformance.
- Issue #40's native-only production, remote CI, cross-platform, and release
  gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-786.md`

## Verification

Passed locally:

- `rustfmt --edition 2024 --check crates/glass-browser/src/browser/native_engine/javascript.rs crates/glass-browser/tests/native_engine.rs`
- `cargo check -p glass-browser --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_local_shared_worker_reuses_named_runtime_and_ports --exact --test-threads=1`
  (1 passed; two connections verify worker-realm `MessageEvent` shape, frozen
  port/source identity, exact-false handler state before later listeners, and
  existing port request/reply behavior)
- `python3 scripts/check-documentation-coverage.py`
  (1,414 Markdown files; coverage validated)
- `git diff --check`

The scoped Cargo check/test pass with existing dead-code warnings in the
native DOM module. Remote CI, full worker/WPT conformance, cross-platform
certification, and issue #40 native-only production gates remain open.

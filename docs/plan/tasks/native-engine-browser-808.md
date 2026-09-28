---
id: native-engine-browser-808
scope: glass-browser/shared-worker-message-port-bridge-close-process-coverage
status: done
depends-on: [native-engine-browser-806]
---

# Glass native-engine browser slice 808: SharedWorker MessagePort bridge close coverage

## Objective

Prove the Slice 806 `NativeWorkerRegistry` bridge-close path with a real
HTTP-loaded SharedWorker and multiple live connections, including both close
directions and isolation of an unaffected connection.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard's MessagePort disentangle steps](https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports)
  sever one channel and fire one `close` Event at the other endpoint.
- Slice [806](native-engine-browser-806.md) implements page/Worker close
  routing through `NativeWorkerRegistry` and has process-backed Dedicated
  Worker evidence. Its fixture deliberately omitted SharedWorker connections.
  SharedWorker connections use the same runtime and route-owner registry, but
  that code-path relationship is not itself conformance evidence.
- A SharedWorker's distinct transferred connection ports share a Worker ID.
  The host route key must continue to isolate each port: closing one route
  cannot tear down another connection or the shared runtime.

## Contract

- Scope is process-backed HTTP verification of explicit `MessagePort.close()`
  on multiple connections to one named SharedWorker through
  `NativeWorkerRegistry`. Do not change Service Worker ownership or use the
  Service Worker registry as a fallback.
- Create at least three connections to one HTTP-loaded SharedWorker and prove
  they reach the same runtime. Use one connection for page-initiated close,
  another for SharedWorker-initiated close, and a third as an independent live
  connection after both closures.
- Validate close in both directions: only the initiating endpoint becomes
  closed, the surviving endpoint remains open but disentangled, and it
  receives exactly one generic `Event` named `close` through `onclose` and
  registered listeners. Check event identity, target/currentTarget, dispatch
  phase, callback `this`, and post-dispatch reset. Repeated close is
  idempotent; the initiator does not receive its own close Event.
- Verify route isolation: purge a same-turn queued message on the connection
  being closed, suppress all later messages on that key, and continue a
  request/reply exchange on the third connection. The SharedWorker runtime
  must remain usable; closing one connection must not terminate its other
  ports or connection handlers.
- Add a deterministic `NativeEngine` process-backed regression using the
  existing local HTTP fixture pattern. Do not infer SharedWorker behavior
  from an inline runtime or from the Dedicated Worker test.
- Change `NativeWorkerRegistry` only if this real process-backed test exposes
  an owner/routing defect. Keep any fix limited to the shared worker close
  path and document the observed failing behavior and exact correction.
- Full SharedWorker lifetime rules, connection shutdown/GC semantics,
  task-source ordering, complete EventTarget/Web IDL/WPT conformance, remote
  CI, and cross-platform certification remain separate issue #40 gates.

## Boundaries and tradeoffs

- Three live connections make cross-route isolation observable while testing
  both close directions in one worker runtime. The local fixture proves only
  this native HTTP path; it does not certify multiple pages or cross-process
  sharing of a SharedWorker.
- This slice is principally a missing regression, not evidence that the
  entire SharedWorker implementation is complete. Do not broaden it into
  SharedWorker lifecycle, scheduling, module, or WPT work unless the close
  test identifies a direct prerequisite.

## Result

The process-backed regression covers three connections to one named
HTTP-loaded SharedWorker runtime. It verifies page- and worker-initiated close,
generic close Event identity and dispatch state, repeated-close idempotence,
initiator silence, route-specific queue purge and post-close suppression, and
a request/reply on the third connection after both closures. The existing
`NativeWorkerRegistry` path passed without a runtime code change.

## Paths

- `crates/glass-browser/tests/native_engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs` (only if a
  SharedWorker close defect is demonstrated)
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`

## Verification

The scoped package check passed:
`cargo check -p glass-browser --test native_engine --locked --quiet` (exit 0;
existing unused HTML-parser warnings only). The exact process-backed test
passed: `cargo test -p glass-browser --test native_engine native_content_process_shared_worker_message_port_bridge_close_both_directions --locked --quiet -- --exact`
(1 passed, 860 filtered; 29.63 seconds). `cargo fmt --all` and
`git diff --check` passed. Remote CI, WPT, and cross-platform certification
remain separate issue #40 gates.

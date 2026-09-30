id: native-engine-browser-840
scope: glass-browser/native-engine/async-worker-effect-delivery
status: done
depends-on: []
---

# Glass native-engine browser slice 840: asynchronous worker-effect transport

## Objective

Add bounded, sequenced asynchronous-effect notifications between the content
process and its owning `NativeEngine`. The transport must wake the parent while
the child has no active request, preserve exact context/frame identity, and
leave the ordinary script response as the sole typed transfer path for the
effects themselves. Backend-wide routing and callback dispatch are Slice 841.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-839.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML event loops](https://html.spec.whatwg.org/multipage/webappapis.html#event-loops)
- [Service Workers](https://www.w3.org/TR/service-workers/)

## Contract

- A worker timer or retained ServiceWorker lifetime continuation that emits a
  browser-facing effect must transfer it to the parent promptly while the
  child has no active request. It is not sufficient to retain effects until a
  later script/evaluation response.
- Parent stdout has exactly one frame reader. It distinguishes request replies,
  the existing synchronous `dialog_open` exchange, and bounded asynchronous
  effect frames. Unsolicited frames carry a strictly increasing per-process
  sequence; malformed, duplicated, out-of-order, oversized, or unknown frames
  fail the content process explicitly.
- The notification preserves owner context/frame identity and does not carry
  page data or duplicate effects. The following ordinary script turn transfers
  queued DedicatedWorker/SharedWorker messages, MessagePort traffic/commands,
  ServiceWorker client/open-window requests, page navigation/close effects,
  and cookie changes through their existing typed response fields. Routing
  must never infer ownership from the currently selected target.
- Backpressure is bounded end-to-end. Once an effect-ready notification is
  outstanding, independent lifetime/timer turns pause until a script turn
  acknowledges the queue. Discrete ordered effects are never dropped,
  coalesced, or executed twice to recover from a full queue; cookie changes
  retain their existing key-coalesced journal semantics.
- `NativeEngine` exposes accepted event notifications to its owner. Backend
  routing, page callback execution, cascade processing, and runtime-owned pump
  lifecycle are specified in [Slice 841](native-engine-browser-841.md), which
  depends on this transport.
- Process-backed tests for delivery without a later browser request, exact
  target/frame routing, cascades, Web Platform Test selection, and remote
  platform evidence belong to Slice 841 and must be recorded there.

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `docs/plan/tasks/native-engine-browser-840.md`

## Verification

- Socket-free protocol tests cover strict owner/sequence validation, unknown
  fields, one outstanding event until script-turn acknowledgement, bounded
  worker-effect queues, ordered output frames, and reader shutdown. Slice 839's
  dialog-input routing regression remains separate and unchanged.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q`
- `cargo fmt --all -- --check`
- `git diff --check`

## Current Evidence

- The content child now emits a strictly sequenced, owner-bound effect-ready
  frame and pauses independent timer/lifetime turns until a script turn
  acknowledges it. Parent stdout has one reader with a one-frame bounded
  handoff; `NativeEngine` exposes the validated notification to its owner.
- The four focused `content_async_effect` unit tests pass. The scoped native
  engine check, formatting, diff check, release-truth audit, documentation
  depth, shortcut inventory, and documentation coverage checks pass.
- This slice establishes transport only. Backend owner wakeup, automatic page
  callback delivery, process-backed routing, WPT evidence, and remote CI remain
  open in Slice 841 and issue #40.
- Issue #40 remains open.

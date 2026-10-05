# Native engine browser slice 847: timer stylesheet load event handoff

```yaml
id: native-engine-browser-847
scope: native-engine/content-process-event-loop
status: in-progress
depends-on: [native-engine-browser-846]
```

## Objective

Deliver the successful `load` event for a stylesheet created by an autonomous
page timer, and preserve network effects produced by its callback through the
parent broker and authoritative cookie jar.

## Context

- `docs/architecture/native-engine.md` — exact-owner timer turns, resource
  loading, event dispatch, and parent effect processing.
- `docs/plan/native-engine-browser-profile.md` — parent-owned cookie contract
  and HTTP(S) resource ownership.
- `docs/plan/tasks/native-engine-browser-846.md` — the passing timer-to-
  stylesheet/import path and the observed missing `load` callback Fetch.
- `crates/glass-browser/src/browser/native_engine/content_process.rs` —
  dynamic stylesheet loading and host event/effect processing.
- `crates/glass-browser/src/browser/native_engine/javascript.rs` — persistent
  page event-handler ownership and callback dispatch.
- `crates/glass-browser/src/browser/native_engine/engine.rs` and
  `crates/glass-browser/src/browser/native_backend.rs` — exact-owner async
  turns and parent effect cascade.
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- After navigation returns, a due page timer may create an HTTP(S) stylesheet
  with an event handler. The handler must run on the same live page owner after
  the stylesheet and its CSS imports have completed.
- Successful loads dispatch one `load` event. Failed or blocked loads dispatch
  one `error` event and must not dispatch `load`.
- A Fetch emitted by the handler must complete through the browser parent's
  normal effect cascade. The content process must not retry HTTP(S) directly.
- The process-backed regression verifies request order and parent cookie
  authority across the page, timer-created stylesheet, imported stylesheet,
  and handler Fetch. Each parent-accepted HttpOnly response cookie must reach
  the next request while `document.cookie` remains empty.
- The test makes no BrowserSession call between navigation and completion of
  the callback Fetch. It uses the native backend and a real loopback server.
- Navigation, close, process failure, and owner replacement must not dispatch
  stale resource events into a replacement document.
- This slice does not claim browser-wide task-source fairness, rendering
  opportunities, or cross-platform conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-847.md`

## Verification

- Scoped `cargo check` for the native-engine integration test target before
  running the focused process-backed regression.
- Explicitly build `glass-native-content-worker` after the scoped check.
- The focused regression verifies one successful load callback, no error
  callback, and its parent-brokered Fetch/cookie chain after navigation
  returns.
- Re-run the Slice 846 idle-timer stylesheet/import regression as the adjacent
  regression.
- `cargo fmt --all -- --check`
- `git diff --check`
- Run the documentation release-truth, depth, shortcut, and coverage checks.

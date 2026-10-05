---
id: native-engine-browser-849
scope: glass-browser/native-engine/autonomous-worker-stream-upload-parent-authority
status: done
depends-on: [native-engine-browser-843, native-engine-browser-848]
---

# Glass native-engine browser slice 849: autonomous Worker stream uploads

## Objective

Close the uncovered DedicatedWorker autonomous-event request path: a due
worker timer must be able to issue a Fetch with a `ReadableStream` request body
through the exact page owner's parent broker, without transferring cookie
authority or falling back to child HTTP(S) transport.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-843.md` — parent-only cookie authority
  and its previously identified autonomous Worker upload gap, closed here for
  the DedicatedWorker timer case only.
- `docs/plan/tasks/native-engine-browser-846.md` — parent-dispatched timer
  owner turns.
- `docs/plan/tasks/native-engine-browser-848.md` — resource callback effects
  remain in their captured owner turn.
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- A due DedicatedWorker timer may upload a bounded `ReadableStream` body to an
  HTTP(S) target while the page is otherwise idle. The event runs only in the
  captured context/frame/document owner turn.
- Upload pull callbacks cross the existing demand-driven, bounded IPC chunk
  path and are collected under current byte and chunk limits before parent
  network dispatch. The request preserves method, content type, credentials
  mode, and body bytes. This does not claim socket-level upload streaming or
  network backpressure.
- The parent alone matches request cookies, accepts `Set-Cookie`, and persists
  the jar. The regression verifies an HttpOnly seed, same-turn visible cookie
  write, parent response-cookie rotation, and use of that rotation on a later
  Worker Fetch; HttpOnly values remain absent from `document.cookie`.
- Existing fail-closed owner checks and the child HTTP(S) transport guard
  remain unchanged; this slice adds no child-side retry or fallback. Dedicated
  stale-owner and cancellation cases are not newly certified here.
- The process-backed regression uses a real loopback server and verifies the
  upload method/body and exact request order, including that no browser-session
  operation is needed to cause the timer turn to run.
- This slice does not close the broader network-authority audit or certify
  Fetch streaming/backpressure, WPT, remote CI, or cross-platform behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs` — repair the existing
  test-only import that blocked library unit-test compilation.
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-843.md`
- `docs/plan/tasks/native-engine-browser-849.md`
- `docs/plan/reviews/native-engine-browser-849-01.md`

## Verification

- `cargo check -p glass-browser --features native-engine --lib --tests
  --locked --quiet` — passed with existing dead-code warnings. The initial
  attempt exposed a pre-existing test-only import error, corrected in
  `native_backend.rs`.
- `cargo build -p glass-browser --features native-engine --bin
  glass-native-content-worker --locked --quiet` — passed.
- `cargo test -p glass-browser --features native-engine --lib
  timer_fetch_uses_parent_cookie_authority --locked -- --nocapture` — passed
  (2 passed; 1,692 filtered; 21.20 seconds), covering DedicatedWorker and
  ServiceWorker timer-cookie regressions.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Documentation checks passed after the docs updates: 1,481 Markdown files,
  zero current-claim failures; 93 current guides and 19 contracts; 15 TUI
  implementation keys and 63 documentation markers; 346 full-product MCP
  tools (101 browser-only), 17 examples, and 22 public modules.
- Review owner checks and parent-cookie flow directly; no independent agent
  review is required for this checkpoint.

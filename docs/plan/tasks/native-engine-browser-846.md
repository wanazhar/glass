# Native engine browser slice 846: autonomous page timer owner turns

```yaml
id: native-engine-browser-846
scope: native-engine/content-process-event-loop
status: in-progress
depends-on: [native-engine-browser-845]
```

## Objective

Advance due timers in an idle process-backed HTTP(S) page without requiring a
later BrowserSession operation. Execute timer callbacks in the exact live page
owner turn, so HTTP(S) Fetches and resources they create continue through the
parent broker and its authoritative cookie jar.

## Context

- `docs/architecture/native-engine.md` — page scheduling, async-effect pump,
  parent broker, and the earlier lack of an autonomous page event loop.
- `docs/plan/native-engine-browser-profile.md` — runtime ordering, parent cookie
  authority, and bounded network/resource contracts.
- `docs/plan/tasks/native-engine-browser-843.md` — child HTTP(S) transport
  fails closed unless owner-bound parent brokering is active.
- `docs/plan/tasks/native-engine-browser-845.md` — backend operation ordering
  and independent owner-turn lifetime.
- `crates/glass-browser/src/browser/native_engine/content_process.rs` —
  content-process timer readiness and script/resource turns.
- `crates/glass-browser/src/browser/native_engine/engine.rs` and
  `crates/glass-browser/src/browser/native_backend.rs` — exact-owner async turn
  dispatch and cancellation.
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- A due page timer in an idle process-backed document emits a bounded
  asynchronous-effect notification; it must not wait for another CLI, SDK, or
  BrowserSession call to make progress.
- The parent validates and dispatches the notification to the live context and
  frame owner. Navigation, close, process failure, and cancellation must not
  let stale timer work mutate a replacement document.
- Timer callbacks keep existing due-time ordering, callback isolation, and
  turn/effect bounds. A timer turn uses the same parent-brokered resource and
  Fetch path as an explicit page-script turn.
- HTTP(S) requests created by the timer carry parent-selected request cookies;
  parent-accepted response cookies are used by dependent resources and later
  requests. HttpOnly values never enter `document.cookie` or child cookie
  state.
- Missing or rejected parent authority fails closed. The sandboxed content
  loader must never retry directly over HTTP(S).
- The process-backed regression uses a real loopback HTTP server and the
  BrowserSession backend pump. It waits for the timer-generated network chain
  after navigation has returned and without issuing a page operation in
  between.
- The regression verifies an idle page timer creating an HTTP stylesheet and
  CSS import. The parent selects the initial HttpOnly cookie for both requests,
  accepts the stylesheet response cookie before the import, retains all three
  cookies, and keeps `document.cookie` empty.
- A diagnostic probe observed that the stylesheet `load` handler did not issue
  its follow-up Fetch. This task does not implement or claim stylesheet load
  event delivery; track that as a separate browser event-loop gap.
- This slice does not claim browser-wide event-loop, rendering-opportunity,
  task-source fairness, or cross-platform conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-846.md`

## Verification

- `cargo check -p glass-browser --features native-engine --test native_engine
  --locked --quiet` — passed.
- `cargo build -p glass-browser --features native-engine --bin
  glass-native-content-worker --locked --quiet` — passed.
- `cargo test -p glass-browser --features native-engine --test native_engine
  --locked native_content_process_idle_page_timer_brokers_dynamic_stylesheet_cookies
  -- --exact --nocapture` — passed (1 passed; 913 filtered; 20.76 seconds).
- The process regression observes `/timer.css` and `/timer-import.css` after
  navigation returns without another BrowserSession operation. It confirms
  parent-selected HttpOnly cookies, parent response-cookie acceptance, and an
  empty `document.cookie` projection.
- `cargo fmt --all -- --check` — passed.
- `git diff --check` — passed.
- Local evidence only; this checkpoint is unpushed and has no remote CI result.

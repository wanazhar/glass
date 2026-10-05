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

- Stylesheet host-event commands now pass through `apply_page_script_evaluation`
  so callback Fetch and other network effects remain queued for the existing
  parent-brokered resolver instead of being treated as document-only commands.
- The focused regression verifies one successful load callback, no error
  callback for the loaded stylesheet, one error callback and no load callback
  for a CSP-blocked stylesheet, and both parent-brokered Fetch chains after
  navigation returns. It verifies the page, stylesheet, CSS import, and
  callback requests in order; parent-accepted HttpOnly cookies reach each next
  request, and `document.cookie` stays empty.
- Re-run the Slice 846 idle-timer stylesheet/import regression as the adjacent
  regression — passed (1 passed; 914 filtered; 32.41 seconds).
- `cargo check -p glass-browser --features native-engine --test native_engine
  --locked --quiet` — passed.
- `cargo build -p glass-browser --features native-engine --bin
  glass-native-content-worker --locked --quiet` — passed.
- `cargo test -p glass-browser --features native-engine --test native_engine
  --locked native_content_process_idle_timer_stylesheet_events_broker_callback_fetches
  -- --exact --nocapture` — passed (1 passed; 914 filtered; 44.68 seconds).
- `cargo fmt --all -- --check` — passed.
- `git diff --check` — passed.
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — passed (1,476 Markdown documents; zero current-claim failures).
- `python3 scripts/check-documentation-depth.py` — passed (93 current guides,
  19 substantive contracts).
- `python3 scripts/check-tui-shortcuts.py` — passed (15 implementation keys,
  63 documentation markers).
- `python3 scripts/check-documentation-coverage.py --glass
  /home/ubuntu/work/glass/target/debug/glass --glass-browser
  /home/ubuntu/work/glass/target/debug/glass-browser` — passed (1,476 Markdown
  files; 346 full-product MCP tools; 101 browser-only tools; 17 examples;
  22 public modules).
- This is local evidence only. The checkpoint is unpushed and has no remote CI
  result; WPT and cross-platform conformance remain unverified.

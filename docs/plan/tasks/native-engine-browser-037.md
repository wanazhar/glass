---
id: native-engine-browser-037
scope: glass-browser/native-engine/page-lifecycle-events
status: done
depends-on: [native-engine-browser-036]
---

# BE-03n/BE-04s: bounded page lifecycle event delivery

## Objective

Deliver the two lifecycle notifications that ordinary page initialization
expects after the bounded script schedule has completed: document
`DOMContentLoaded`, followed by window `load`.

## Contract

- Local and sandboxed child-owned documents dispatch one non-cancelable,
  non-bubbling `DOMContentLoaded` event to `document` after accepted page
  scripts and their bounded task drain complete.
- The same owners then dispatch one non-cancelable, non-bubbling `load` event
  to the window target. Listener registration remains in the persistent page
  realm, so callbacks can publish the existing bounded typed mutations.
- The event targets use the existing typed host-event bridge; no executable
  callback or page object crosses the content-process IPC boundary.
- Lifecycle callback mutations use the same clone-and-commit validation as page
  scripts. Navigation commands during the load-time callback remain rejected
  because navigation ownership begins only after the committed document.

## Deliberate boundary and tradeoffs

- This slice does not claim `document.readyState` transitions, `readystatechange`,
  `beforeunload`, `unload`, `pagehide`, `pageshow`, resource-specific load
  events, completion-order races, or full task-source timing.
- The bounded engine dispatches lifecycle events after its complete discovered
  script schedule. It does not model incremental HTML parsing, dynamic script
  insertion, parser interruption, or a browser's live network event loop.
- Lifecycle events are not added to the privacy-preserving Rust effect log;
  only callback mutations are committed, avoiding raw page state in effects.

## Paths

- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_lifecycle_events_fire_after_script_schedule -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_runs_inline_page_scripts_in_persistent_realm -- --nocapture` — 1/1 passed with child lifecycle assertions.
- `git diff --check`

Remote CI, push, release, tag, registry publication, browser-parity, and
issue-closure claims remain pending the wider browser-complete gates.

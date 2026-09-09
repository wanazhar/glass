---
id: native-engine-browser-039
scope: glass-browser/native-engine/document-ready-state
status: done
depends-on: [native-engine-browser-038]
---

# BE-03p/BE-04u: bounded document ready-state lifecycle

## Objective

Make the lifecycle events delivered by task 037 carry a useful document state
contract, including pages that have no discovered scripts.

## Contract

- A local or child-owned page starts script evaluation in `document.readyState`
  `loading`, transitions to `interactive` after the accepted script schedule,
  and transitions to `complete` before the window `load` event.
- The owner dispatches document `readystatechange` at the interactive and
  complete transitions, then document `DOMContentLoaded`, then window `load`.
  Listener callbacks see the corresponding ready-state value and retain
  mutations through the existing typed clone-and-commit path.
- A page without scripts still receives a persistent native JavaScript realm
  with final `document.readyState === "complete"`; later evaluation does not
  fail merely because no page script was discovered.
- Local and child owners use the same phase ordering and typed event metadata;
  no page callback or JavaScript object crosses content-process IPC.

## Deliberate boundary and tradeoffs

- This is a deterministic lifecycle phase model, not incremental HTML parsing
  or a wall-clock network completion model. Resource-specific load events,
  stylesheet/image/font/media completion, completion races, parser
  interruption, dynamic insertion, and full task-source scheduling remain
  open.
- `beforeunload`, `unload`, `pagehide`, `pageshow`, BFCache, document.open,
  prerendering, and navigation cancellation remain outside this bounded slice.
- The runtime keeps one owner-local phase string and recreates only the host
  snapshot around each evaluation; it does not claim live Web IDL identity.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_pages_without_scripts_retain_complete_ready_state -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_runs_inline_page_scripts_in_persistent_realm -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_evaluates_persistent_script_realm -- --nocapture` — 1/1 passed.
- `git diff --check`

Remote CI, push, release, tag, registry publication, browser-parity, and
issue-closure claims remain pending the wider browser-complete gates.

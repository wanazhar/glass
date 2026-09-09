---
id: native-engine-browser-047
scope: glass-browser/native-engine/timer-ordering
status: done
depends-on: [native-engine-browser-046]
---

# BE-01k/BE-04ac: bounded due-time timer turns

## Objective

Make `setTimeout` honor its bounded delay instead of draining every pending
timer on the next evaluation, while retaining deterministic callback ordering.

## Contract

- Timer delays are normalized to a non-negative bounded interval and stored
  with a monotonic host due time in both local and child-owned realms.
- A host turn drains only due timers, ordered by due time and then timer ID;
  zero-delay timers remain next-turn work rather than running during the
  scheduling call.
- `clearTimeout` removes a pending timer before its due turn.
- Callback state remains inside the persistent realm; only typed DOM commands
  cross the existing owner boundary.
- The same behavior is available for local and process-backed documents.

## Deliberate boundary and tradeoffs

This adds due-time checks at host turns; it does not create a background page
event loop. A callback therefore requires a later Glass host evaluation or
action to be observed. `setInterval`, animation/idle callbacks, task-source
fairness, cancellation across outstanding network work, and full wall-clock
browser scheduling remain open. The monotonic clock is realm-local and is not a
public wall-clock API.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_local_delayed_timer_waits_for_due_host_turn`
- `native_content_process_delayed_timer_waits_for_due_host_turn`
- existing zero-delay microtask/timer regression
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.

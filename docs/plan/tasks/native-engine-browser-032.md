---
id: native-engine-browser-032
scope: glass-browser/native-engine/task-turns
status: done
depends-on: [native-engine-browser-031]
---

# BE-01i/BE-04n: bounded JavaScript task turns

## Objective

Give the persistent native page realm a deterministic microtask and next-turn
timer boundary so ordinary promise callbacks and simple delayed work have an
observable ordering contract.

## Contract

- `queueMicrotask` schedules a callback on the current QuickJS job queue;
  pending jobs are drained after each bounded JavaScript evaluation and event
  callback turn.
- `setTimeout` and `clearTimeout` are exposed with a bounded callback quota.
  Accepted callbacks are retained in the page realm and run at the beginning
  of the next native host turn, before that turn's script/evaluate or event
  callback. Timer callback arguments and typed host commands remain in the
  realm; no raw callback crosses IPC.
- Full navigation creates a new timer registry. Same-document navigation and
  later child/local actions retain the existing registry with the existing
  realm.

## Deliberate boundary and tradeoffs

- This is deterministic turn ordering, not wall-clock scheduling. Delay values
  are accepted as bounded host-turn metadata but do not create a real-time
  sleep. `setInterval`, animation frames, idle callbacks, background throttling,
  task-source prioritization, and parser timing remain open.
- The native queue is bounded to the existing host-effect budget. A page that
  continually schedules timers or microtasks is stopped by the same runtime
  interrupt/memory/stack limits rather than receiving an unbounded event loop.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine next_turn_timers` — 1/1 passed.
- `git diff --check`

The next runtime/document gates are parser-blocking/defer/async ordering,
submission lifecycle/default actions, and the remaining browser contexts.

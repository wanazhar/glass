# Native content task-source round-robin scheduling (357)

```yaml
id: native-engine-browser-357
scope: native-engine/content-task-source-scheduling
status: done
depends-on:
  - native-engine-browser-356
```

## Objective

Give a content-process script turn an explicit, bounded policy for choosing
among ready browser task sources. The policy must preserve source-local FIFO
ordering, prevent a continuously ready networking queue from starving a due
timer, and keep JavaScript microtask checkpoints attached to the persistent
runtime that owns the turn.

## Delivered behavior

- Added five explicit content task sources: Networking, WebSocket,
  FetchStream, EventSource, and Timer.
- Select at most one ready host task per scheduler cycle and rotate a
  round-robin cursor across those sources.
- Preserve the Networking source's script-emission FIFO queue while retaining
  the existing response and Promise-continuation handoff.
- Suspend the implicit timer pump for the host-task loop. Timer work runs only
  when the scheduler explicitly selects the Timer source, and the guard
  restores the prior runtime state on every exit path.
- Leave the persistent JavaScript owner responsible for the microtask drain
  after each selected host task.
- Add an HTTP-backed witness in which two Fetch continuations and a due
  zero-delay timer complete in `first`, `timer`, `second` order, alongside the
  existing FIFO and timer-started-Fetch coverage.

## Contract and tradeoffs

The HTML Standard models separate task queues for task sources and requires
ordering within a source, while allowing the user agent to choose among
sources. The round-robin choice is therefore an explicit native-engine
product policy, not a claim that one cross-source order is mandated by the
platform. It gives a continuously ready networking source a bounded turn
share, but it does not make host I/O physically concurrent: response delivery
and script continuation remain serialized through the content-process owner.

This scheduler is local to one content-process script turn. Worker,
ServiceWorker, MessagePort, rendering, navigation, and browser-wide queues
still need their own ownership and cross-process arbitration before the native
engine can claim complete browser task scheduling. The timer-pump guard avoids
bootstrap reentrancy; explicit timer turns retain the existing timer
deadline, callback, and microtask behavior.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`

## Verification

All commands ran locally against the current checkout:

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo check --quiet -p glass-browser --lib --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_round_robins_due_timer_between_network_tasks --locked -- --exact --nocapture` — 1 passed
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_preserves_fifo_network_task_order --locked -- --exact --nocapture` — 1 passed
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_drives_timer_started_fetch_continuation --locked -- --exact --nocapture` — 1 passed
- `git diff --check`
- documentation coverage, depth, release-truth, and TUI-shortcut validators

No remote CI, push, release, tag, or publication claim is made by this task.

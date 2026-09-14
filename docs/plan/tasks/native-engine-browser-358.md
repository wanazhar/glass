# Native browser-effect source arbitration (358)

```yaml
id: native-engine-browser-358
scope: native-engine/browser-effect-source-arbitration
status: done
depends-on:
  - native-engine-browser-357
```

## Objective

Give browser-owned popup, postMessage, navigation, and close effects an
explicit bounded dispatch policy. A popup cascade must not drain the other
browser-effect sources first, and each source must retain its existing FIFO
order and nested-effect behavior.

## Delivered behavior

- Added four browser-effect sources: Popup, Message, Navigation, and Close.
- Select at most one ready effect per dispatch cycle through a rotating
  round-robin cursor, skipping empty queues without changing source-local
  order.
- Persist the cursor on `NativeEngineBackend` so separate dispatch calls do
  not reset to popup priority when more than one source remains active.
- Re-enqueue nested effects into their original source queues after popup
  creation, target navigation, or message delivery, preserving the existing
  bounded cascade limit and cleanup behavior.
- Keep close effects last in the declared source cycle while still allowing a
  close request to run before a newly generated popup when its turn is due.
- Restore the native resource-loader unit-test import required for the
  library-test target to compile; this is a test-only repair discovered while
  validating the scheduler.

## Contract and tradeoffs

The source order is an explicit Glass policy: the platform permits user-agent
choice between distinct task sources, while source-local ordering remains
stable. Persisting the cursor prevents repeated calls from reintroducing a
fixed popup-first priority, but the four queues are still drained at an
operation boundary; this slice does not create an autonomous browser-wide
event loop or durable idle task store.

Dispatch remains serialized through the backend owner. That preserves target
topology, origin checks, navigation transactionality, and rollback semantics,
but a slow target operation can still delay later effects. Worker,
ServiceWorker, MessagePort, rendering, navigation-task, registration, and
cross-process client queues still need their own ownership and conformance
work before issue #40 can claim complete browser scheduling.

## Touched paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-358.md`

## Verification

All commands ran locally against the current checkout:

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo check --quiet -p glass-browser --lib --bins --test native_engine --locked`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo test --quiet -p glass-browser --lib native_browser_effect_sources --locked` — 2 passed
- `git diff --check`
- documentation coverage, depth, release-truth, and TUI-shortcut validators

No remote CI, push, release, tag, or publication claim is made by this task.

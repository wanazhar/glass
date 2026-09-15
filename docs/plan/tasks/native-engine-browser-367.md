# Native worker timer task-source fairness (367)

```yaml
id: native-engine-browser-367
scope: native-engine/worker-task-sources
status: done
depends-on:
  - native-engine-browser-366
```

## Objective

Prevent one dedicated or shared worker from monopolizing a page operation by
running every due worker timer before the page owner gets a turn.

## Delivered behavior

- The native worker registry keeps a stable rotating worker-timer cursor.
- Each host boundary executes at most one due worker timer turn.
- Worker ids are visited in ascending order after the cursor and wrap to the
  lowest live id, so worker creation/removal does not permanently bias the
  first worker.
- Existing isolated worker evaluation, message/error routing, fetch handling,
  MessagePort routing, and timer limits remain unchanged.
- The deterministic local engine witness proves two simultaneously due
  workers deliver one timer result per successive page turn in round-robin
  order.

## Contract and tradeoffs

The unit of fairness is one worker realm's timer turn, not one callback: a
single realm may still drain its own due timer queue according to the existing
bounded runtime contract. This keeps callbacks serialized and limits host
overhead, while preventing a continuously busy low-id worker from starving
other workers. Worker callbacks remain operation-boundary work; this slice
does not add a resident background loop or claim complete ordering with page,
Service Worker, MessagePort, rendering, or browser-context task sources.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-367.md`

## Verification

The following checks passed locally against the current checkout:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_worker_timers --locked` — 2 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.

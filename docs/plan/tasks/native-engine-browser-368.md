# Native Service Worker timer task-source admission (368)

```yaml
id: native-engine-browser-368
scope: native-engine/service-worker-task-sources
status: done
depends-on:
  - native-engine-browser-367
```

## Objective

Admit due timers created by active or waiting Service Worker realms at the
next page host boundary. A timer must not remain permanently pending merely
because the worker is isolated from the page runtime.

## Delivered behavior

- The native JavaScript runtime can execute one Service Worker timer turn with
  the complete Service Worker host surface installed.
- The Service Worker registry keeps a rotating worker-id cursor shared by
  active and waiting workers.
- Each page host boundary admits at most one due Service Worker timer realm;
  worker ids are visited after the cursor and wrap over the live set.
- Timer-produced client messages, MessagePort commands, Cache API work, and
  other already-supported bounded worker commands pass through the existing
  settlement and queue owners.
- The content-process script host admits the Service Worker timer turn before
  draining queued page-facing worker, MessagePort, and client messages.
- A process-backed HTTP(S) witness proves that a Service Worker `setTimeout`
  callback posts to the page on the following page evaluation.

## Contract and tradeoffs

The admission unit is one Service Worker realm timer turn, not one callback;
the realm may still batch its own due timers under the existing bounded runtime
contract. Active and waiting registrations share fairness, but this slice does
not claim one global ordering across page timers, dedicated/shared workers,
networking, rendering, or browser-context effects. Delivery remains tied to an
explicit page host operation, which avoids a resident background loop and
keeps DOM mutations serialized on the content owner. Timer evaluation errors
continue through the existing Service Worker host error boundary rather than
being silently discarded.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-368.md`

## Verification

The following checks passed locally against the current checkout:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_runs_service_worker_timer_on_next_page_turn --locked` — 1 passed
- Service Worker-filtered native integration binary — 16 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.

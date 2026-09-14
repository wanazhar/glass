# Service Worker `clients.openWindow()` target materialization (359)

```yaml
id: native-engine-browser-359
scope: native-engine/service-worker-open-window
status: done
depends-on:
  - native-engine-browser-358
```

## Objective

Connect the native Service Worker `clients.openWindow()` surface to the
browser-owned target model. A successful call from a non-fetch Service Worker
turn must create a real same-origin page target, retain the source opener
relationship, and resolve the original Promise with a WindowClient-shaped
descriptor that reflects the new target.

## Delivered behavior

- Added a validated `ServiceWorkerOpenWindow` command and a bounded pending
  request queue with stable worker/request identity.
- Kept the request and its Promise continuation in the persistent worker
  runtime while the browser backend creates the target.
- Carried the request across the content-process IPC boundary and routed the
  resolution back to the exact source context and frame.
- Created a real parked native page target for the resolved HTTP(S),
  same-origin URL, with the source target as its opener.
- Synchronised the browser-wide Service Worker client projection before
  resolving the worker Promise, then returned the new target's ID, URL, type,
  frame type, visibility, focus, and `postMessage` surface.
- Sent nested popup, message, close, navigation, and `openWindow` effects back
  through the existing bounded browser-effect scheduler.
- Added an HTTP-backed integration witness covering registration, activation,
  a controlled Service Worker message event, target creation, opener/parked
  state, and descriptor observation from a later worker Fetch.

## Contract and tradeoffs

The native browser owns target creation and Promise settlement; the content
process does not fabricate a second page or silently invoke CDP. URLs are
limited to HTTP(S), same-origin targets without credentials, and fragments are
removed before target creation. New windows are parked so the source page
keeps focus, matching Glass's existing popup ownership contract.

The pending request is bounded and deduplicated when crossing the content /
browser boundary. The effect scheduler remains serialized at the backend
topology owner, which preserves target identity and origin checks but does not
provide physically concurrent target creation. Fetch-event suspension and
arbitrary-target `WindowClient.postMessage()` routing remain separate
Service Worker queue/conformance work; this slice does not claim those paths.

## Touched paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-359.md`

## Verification

All commands ran locally against the current checkout:

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo check --quiet -p glass-browser --lib --bins --test native_engine --locked`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_service_worker --locked` — 2 passed
- `CARGO_PROFILE_DEV_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_runtime_service_worker_clients_open_window_materializes_window_client --locked` — 1 passed
- `git diff --check`
- documentation coverage, depth, release-truth, and TUI-shortcut validators

No remote CI, push, release, tag, or publication claim is made by this task.

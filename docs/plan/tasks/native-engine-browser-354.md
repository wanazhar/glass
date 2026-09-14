# Native Service Worker multi-client topology (354)

```yaml
id: native-engine-browser-354
scope: native-engine/service-worker-multi-client-topology
status: done
depends-on: [native-engine-browser-353]
```

## Objective

Promote the native Service Worker client surface from one process-local
top-level document to a bounded browser-wide projection:

- assign stable opaque client identities from the native target/frame owner;
- synchronize live top-level targets and attached nested frames into every
  content-process Service Worker registry;
- preserve client type, frame type, URL, visibility, and focus metadata;
- make `clients.matchAll()` enumerate same-origin clients across target and
  frame owners, with `type` and `includeUncontrolled` filtering; and
- carry the same client projection into `FetchEvent` dispatch and worker
  lifecycle/message/cache evaluations.

This slice does not claim cross-process `Client.postMessage` delivery,
`clients.openWindow()`, browser-wide registration/update arbitration, exact
HTML task-source scheduling, or durable live-client leases. Those remain
issue #40 promotion gates.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-353.md`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`

## Path

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-354.md`

## Contract and tradeoffs

The semantic native backend reconciles known target/frame owners before a
normal operation, constructs a deterministic bounded client list, and sends
that list to each live content process only when that owner’s projection has
changed. A top-level target is represented as `frameType: "top-level"`; an
attached frame is `frameType: "nested"`. Only the selected active frame is
focused; parked targets remain unfocused. The client id is derived from the
native context/frame identity, so replacing a content process does not create a
second identity for the same live frame.

Each content process validates the list at the IPC boundary and its Service
Worker registry calculates same-origin control status per registration. The
worker bootstrap receives the projection for lifecycle, message, cache, and
fetch turns. Fetch payloads also carry the current client envelope, preserving
`FetchEvent.clientId` and current-client control semantics while
`clients.matchAll()` can enumerate the other live clients.

The projection is bounded by the existing target/frame topology limits and the
existing script/IPC budgets. Synchronizing all known owners adds work to a
normal backend dispatch, but equality checks avoid repeating IPC when the
projection is unchanged. Registration profiles still need a shared persistent
or browser-context profile for independent content processes to restore the
same Service Worker; this slice does not invent a second registration store.

## Verification

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo check --quiet -p glass-browser --lib --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine service_worker --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_runtime_service_worker_clients_match_all_across_targets --locked`
- `git diff --check`
- repository documentation coverage, depth, release-truth, and TUI shortcut
  validators

All evidence is local unless a later explicitly authorized push produces
remote CI evidence.

## Local evidence

- `cargo fmt --all -- --check` — passed
- `CARGO_PROFILE_DEV_DEBUG=0 cargo check --quiet -p glass-browser --lib --bins --test native_engine --locked` — passed
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine service_worker --locked` — passed
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_runtime_service_worker_clients_match_all_across_targets --locked` — passed; two same-origin top-level targets returned distinct client ids and both URLs through `clients.matchAll()`
- `git diff --check` — passed

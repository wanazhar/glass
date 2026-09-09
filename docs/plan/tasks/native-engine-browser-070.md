---
id: native-engine-browser-070
scope: glass-browser/native-engine/local-storage-event-delivery
status: done
depends-on: [native-engine-browser-069]
---

# BE-02/BE-03/BE-04/BE-07: bounded same-profile local storage events

## Objective

Deliver the first cross-document page `storage` event contract for local
native documents that are alive concurrently and share the same explicit
profile path. Keep the event source document excluded, preserve origin
partitioning, and synchronize the receiving document's Rust-owned storage
state before dispatching the event into its persistent JavaScript realm.

## Contract

- A native engine with an explicit `storage_path` subscribes to the bounded
  in-process coordinator for that profile path.
- `localStorage.setItem`, `removeItem`, and `clear` publish only effective
  mutations. Same-value writes, removing an absent key, and clearing an empty
  store publish no event.
- Other live native documents receive only events whose origin-keyed storage
  identity matches their current document. The source document never receives
  its own event.
- Receiving a set/remove/clear event updates the Rust-owned local-storage
  state, the JavaScript storage map, and a bounded `StorageEvent` descriptor
  with `key`, `oldValue`, `newValue`, `url`, and `storageArea` before listener
  dispatch.
- Event callbacks can issue bounded local-storage writes; those writes follow
  the same persistence and fan-out path.
- Subscriber and pending-event queues are bounded by the existing native
  effect limit. Queue overflow is reported as a typed native error.

## Deliberate boundary and tradeoffs

This slice is deliberately local and in-process. It does not claim event
transport into the sandboxed HTTP(S) content worker, cross-process/profile
locking, iframe or popup browsing-context identity, or session-storage event
routing. `sessionStorage` remains session-scoped and is not broadcast between
independent `NativeEngine` owners because the current model has no
top-level-browsing-context identity. IndexedDB, full Storage Web IDL identity,
and concurrent profile-writer coordination remain open.

The event queue is drained when a receiving engine evaluates script. That is a
bounded host scheduling contract, not a claim of browser task-source or
microtask ordering parity.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- Implementation checkpoints: `b329df8f` and `7a2e4117`
- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test -p glass-browser --features native-engine --test native_engine native_local_web_storage_delivers_origin_filtered_events_to_other_documents -- --nocapture` — 1 passed, 359 filtered
- `cargo build --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 360 passed, 0 failed

Remote CI remains pending this checkpoint. Source push, release, tag, registry
publication, and issue-closure claims remain out of scope for this local slice.

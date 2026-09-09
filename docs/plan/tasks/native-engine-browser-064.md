---
id: native-engine-browser-064
scope: glass-browser/native-engine/semantic-storage-contract
status: done
depends-on: [native-engine-browser-063]
---

# BE-07: bounded semantic storage contract

## Objective

Make the native backend execute the existing `StorageRequest` contract for
explicit local and session key-value operations, and expose that operation
through `BrowserRuntimeSession`.

## Contract

- `StorageScope::Local` and `StorageScope::Session` support bounded read,
  write, and clear operations through the native dispatcher.
- Local and session state remain separate and are scoped to the backend
  instance; map validation and response limits continue to come from the
  shared backend contract.
- Storage requests require the active native context and return the stable
  `StorageResult` response shape.
- `BrowserRuntimeSession::storage` uses the same dispatcher path as direct
  backend callers.

## Deliberate boundary and tradeoffs

This is backend contract plumbing, not browser storage parity. The state is
not yet visible to page JavaScript, durable across engine instances, keyed by
origin, synchronized with the network cookie jar, or backed by IndexedDB.
Cookie-scope requests remain explicitly unsupported rather than being mapped
to an incorrect key-value representation. Those gaps remain BE-02/BE-07
completion work.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine semantic_storage_uses_bounded_native_backend_state -- --nocapture` — 1 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.

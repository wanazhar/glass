---
id: native-engine-browser-065
scope: glass-browser/native-engine/page-web-storage-realm
status: done
depends-on: [native-engine-browser-064]
---

# BE-02/BE-04/BE-07: bounded page Web Storage realm

## Objective

Expose bounded page-visible `localStorage` and `sessionStorage` objects in the
shared native QuickJS bootstrap used by local documents and the sandboxed
content worker.

## Contract

- Both storage objects expose bounded `length`, `key()`, `getItem()`,
  `setItem()`, `removeItem()`, and `clear()` behavior.
- Keys, values, and total entries use the existing Glass backend limits;
  attempts to exceed a limit fail with a bounded `RangeError` before mutation.
- Values persist across evaluations in the same page realm and remain
  independent between local and session storage.
- The API is installed by the same bootstrap for local and child-owned script
  execution, so the two execution paths have the same bounded behavior.

## Deliberate boundary and tradeoffs

This slice is realm-local Web Storage, not browser storage parity. State is not
yet origin-keyed across navigations, durable across engine instances, exposed
through the semantic `StorageRequest` operation, synchronized with the network
cookie jar, dispatched through `storage` events, or backed by IndexedDB. Full
Storage Web IDL identity and quota policy remain open. The explicit boundary
avoids treating a JavaScript-only map as durable profile state.

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

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_session_uses_explicit_local_constructor -- --nocapture` — 1 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.

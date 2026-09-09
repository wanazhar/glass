---
id: native-engine-browser-066
scope: glass-browser/native-engine/origin-keyed-web-storage
status: done
depends-on: [native-engine-browser-065]
---

# BE-02/BE-03/BE-04/BE-07: origin-keyed page Web Storage transfer

## Objective

Carry bounded page-visible `localStorage` and `sessionStorage` state through
fresh realms created by navigation while keeping the runtime owner and the
two-crate/process boundary explicit.

## Contract

- The QuickJS runtime consumes page storage mutations into its bounded Rust
  owner instead of leaving them only in a JavaScript map.
- Fresh local realms receive the storage state for their current origin before
  page scripts execute.
- The sandboxed content worker retains state across HTTP(S) document loads,
  refreshes it before each new load, and seeds the next child realm without
  sending storage state through the DOM/content wire.
- Tuple origins share their bounded local/session maps across navigation;
  opaque local documents use a fragment-free document key. Local and session
  stores remain independent and keep the existing key, value, and entry caps.

## Deliberate boundary and tradeoffs

State is volatile within the engine/content-worker lifetime and is not durable
across restart or profile recreation. The page-visible maps remain separate
from the semantic `StorageRequest` maps, so this slice does not claim one
unified storage profile. Storage events, cookie synchronization, IndexedDB,
quota policy, partitioning, full Storage Web IDL identity, and browser-wide
profile isolation remain open. The transfer is intentionally runtime-owned so
the DOM wire does not become a second storage authority.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_preserves_origin_keyed_web_storage_across_navigation -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_session_uses_explicit_local_constructor -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_page_web_storage_realm -- --nocapture` — 1 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.

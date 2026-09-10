---
id: native-engine-browser-076
scope: glass-browser/native-engine/cookie-profile-persistence
status: done
depends-on: [native-engine-browser-075]
---

# BE-02/BE-03/BE-04: opt-in cookie profile persistence

## Objective

Persist the native engine's bounded Rust-owned HTTP cookie jar through the
existing explicit `NativeEngineConfig::storage_path` profile. Reopening the
same profile must restore page-visible and request-visible cookies for both
the local owner and the sandboxed content worker, while keeping profile I/O
bounded, locked, atomic, and merge-safe.

## Contract

- A missing `storage_path` keeps the cookie jar volatile. Supplying a path
  opts into one versioned JSON profile containing localStorage and an optional
  bounded cookie array; profiles written before the cookie field remain
  readable.
- Accepted cookies are serialized with their name, value, domain, path,
  host-only, Secure, HttpOnly, and bounded wall-clock expiry metadata. The
  explicit profile lifetime restores session cookies as well as non-expired
  Max-Age cookies; expired entries are discarded on read and write.
- Cookie changes use the existing profile `P.lock`, re-read the latest
  snapshot, and apply bounded key-level replacement/deletion/eviction deltas.
  A complete snapshot is written through the existing atomic rename or
  verified platform fallback, so stale live contexts do not erase unrelated
  cookie keys or localStorage changes.
- Local `document.cookie`, HTTP `Set-Cookie`, navigation, fetch, and all
  content-worker resource paths feed the same mutation journal in the loader;
  profile persistence occurs at navigation/evaluation/IPC/close boundaries.
- The content worker loads the cookie profile when its resource loader is
  initialized and never receives cookies through unbounded IPC. Cookie values
  are not logged or included in diagnostic output.
- The semantic `StorageScope::Cookies` map remains a separate unsupported
  backend operation; this slice covers the page/resource cookie contract, not
  a new public cookie-management API.

## Deliberate boundary and tradeoffs

Persisting session cookies across a deliberate profile restart makes the
explicit path useful as a browser profile, but it means callers must treat the
profile as sensitive credential-bearing state and delete/rotate it when its
session should end. The bounded jar still does not claim full SameSite,
partitioned-cookie, Expires-date, third-party, cookie-change-event, or full
Cookie/Document Web IDL parity. The single profile lock and delta merge add a
small read/serialize/write cost at persistence boundaries, trading that cost
for predictable recovery and protection against stale-snapshot loss.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the implementation and ordered native/docs gates
complete. The focused acceptance gate is a sandboxed content-process restart:
an HTTP `Set-Cookie` and page-set cookie must be present in the next process's
request header and `document.cookie` view. The existing localStorage stale
merge, profile-lock, cross-process event, full native target, documentation,
and release-truth gates must remain green.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine cookie_profile_merges_stale_key_changes -- --nocapture` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_persists_cookie_profile_through_restart -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_web_storage_persists_through_profile_restart -- --nocapture` — 1 passed after the content-worker ownership fix
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 368 passed, 0 failed

The parent engine does not atomically replace a profile while a healthy
sandboxed content worker owns a bound profile view; the worker flushes its
profile before asynchronous close. This preserves the existing Linux sandbox
mount contract while retaining normal parent-owned persistence for local
documents.

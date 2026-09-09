---
id: native-engine-browser-068
scope: glass-browser/native-engine/durable-web-storage-profile
status: done
depends-on: [native-engine-browser-067]
---

# BE-02/BE-03/BE-04/BE-07: opt-in durable page Web Storage profile

## Objective

Persist the bounded origin-keyed page `localStorage` map across native engine
restarts without making the default native runtime write user state. Keep
`sessionStorage` session-scoped, and make the same profile contract apply to
the local QuickJS owner and the sandboxed network content worker.

## Contract

- `NativeEngineConfig::with_storage_path` opts one engine into a bounded JSON
  profile file; the path must be absolute, the default remains volatile, and
  no profile is written unless the option is supplied.
- The profile is versioned, capped at 4 MiB, validates origin/key/value limits,
  creates its parent directory, and replaces a complete temporary snapshot.
- Only origin-keyed page `localStorage` is serialized. `sessionStorage` is
  loaded as an empty session map on every engine start and is never written.
- Local pages seed fresh realms from the profile and persist mutations after
  script evaluation, navigation lifecycle events, same-document events, and
  close.
- The sandboxed content worker receives the configured path over its typed
  IPC start command, loads it before the first document, and saves the latest
  state after worker commands and close. On Linux the sandbox binds only the
  explicit profile file into its private filesystem namespace.

## Deliberate boundary and tradeoffs

The profile path is explicit to avoid silently persisting browsing state. It
is a bounded state snapshot, not a browser profile: there is no multi-writer
lock/merge protocol, storage-event delivery, cookie synchronization, quota
manager, IndexedDB, crash journal, encryption/key management, or full Storage
Web IDL identity. The replacement is atomic where the platform permits
rename-overwrite and uses a bounded complete-snapshot fallback where it does
not; callers must not concurrently write one path from multiple engines.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check` — passed
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo build --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine web_storage_persists_through_profile_restart -- --nocapture` — 2 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_page_web_storage_realm -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_preserves_origin_keyed_web_storage_across_navigation -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_file_and_blob_form_data_are_bounded_and_text_backed -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_uploads_bounded_file_blob_form_data -- --nocapture` — 1 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.

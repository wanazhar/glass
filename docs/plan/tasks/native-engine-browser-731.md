---
id: native-engine-browser-731
scope: glass-browser/public-rust-navigation
status: done
depends-on: [native-engine-browser-730]
---

# Glass native-engine browser slice 731: canonical history and recovery

## Objective

Expose native history traversal, reload, and owner recovery through the
canonical Rust `BrowserSession` API. Exercise the public call path and its
revision/error behavior, without routing native operations through CDP.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/architecture/browser.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/rust-sdk.md`
- `docs/plan/tasks/native-engine-browser-730.md`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/browser/native_engine/history.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- `BrowserSession` exposes `go_back`, `go_back_with_revision`, `go_forward`,
  `go_forward_with_revision`, `reload_with_revision`, `recover`, and
  `recover_with_revision`.
- History, reload, and recovery operations delegate to the existing native
  owner. Revision-checked operations reject stale observations before changing
  browser state. These methods never probe, launch, or fall back to CDP.
- A non-native runtime returns a typed
  `BrowserBackendError::UnsupportedOperation` before native state access.
- Existing `native_*` runtime methods remain source-compatible.
- `stop_loading_with_revision` is intentionally not promoted as a working
  standard operation in this slice: the current native implementation is a
  revision-preserving no-op because navigation completes before its call
  returns. Actual interruption of an in-flight navigation remains open work.

## Path

- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-731.md`
- `docs/features.md`
- `docs/rust-sdk.md`
- `CHANGELOG.md`

## Verification

- Exercise guarded and unguarded history traversal, revision-checked reload,
  and guarded/unguarded recovery through `BrowserSession`.
- Verify stale revisions fail without applying a history, reload, or recovery
  transition.
- Verify all seven standard methods reject a non-native session with the
  matching typed unsupported-operation error and do not switch transports.
- After the coherent code/docs batch, run one locked package-scoped
  `glass-browser` all-target check, then the focused typed-error unit test and
  native history/recovery integration tests. Run formatting, whitespace,
  release-truth, documentation-depth, and shortcut gates. Do not run
  workspace-wide tests, remote CI, `cargo clean`, or mark issue #40 complete.

## Results

- `cargo check --locked --quiet -p glass-browser --all-targets` passed; its
  only diagnostic was an unused import removed before the focused test runs.
- `standard_history_and_recovery_reject_non_native_backend_typed` passed,
  covering all seven methods.
- `canonical_browser_session_exposes` passed both history and recovery
  integration tests, including stale-revision rejection and unchanged state.
- `native_runtime_supports_form_pdf_clipboard_and_consent_surfaces` passed
  with a per-test 8 MiB stack. Its default test-harness stack overflowed before
  the harness was made explicit; the test now passes without a global
  `RUST_MIN_STACK` override.
- `rustfmt --check` on both touched Rust files, `git diff --check`, and the
  static maintainer gates passed: release-truth validated 1,359 Markdown files
  with zero current-claim failures; documentation-depth validated 93 guides
  and 19 contracts; shortcut inventory validated 15 keys and 63 markers.
- Live documentation coverage was skipped because `target/debug/glass` is
  absent. Remote CI and native-only platform certification were not run.
  Issue #40 remains open; in-flight stop-loading cancellation and browser
  completion remain unverified.

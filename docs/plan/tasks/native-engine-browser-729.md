---
id: native-engine-browser-729
scope: glass-browser/public-rust-observation
status: done
depends-on: [native-engine-browser-728]
---

# Glass native-engine browser slice 729: canonical semantic observation API

## Objective

Expose the existing revisioned native semantic observation through standard
`BrowserSession` method names so Rust callers can inspect pages without using
CDP or native-internal method names.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/architecture/browser.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/rust-sdk.md`
- `docs/plan/tasks/native-engine-browser-728.md`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/browser/session/semantic.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- `BrowserSession::observe()` returns the existing bounded
  `SemanticObservation` at the documented structured level.
- `BrowserSession::semantic_observe(level)` returns that exact requested level
  from one revisioned native frame snapshot.
- `BrowserSession::semantic_expand_region(region, revision, level)` preserves
  the existing stale-revision rejection and single-region projection.
- `BrowserSession::inspect_page()` and `observe_bootstrap()` expose the
  existing native inspection/bootstrap envelopes under standard names.
- These methods call the native snapshot owner directly. They do not probe,
  launch, or fall back to CDP. Existing `native_*` spellings remain compatible
  and share the same snapshot owner.
- Firefox/Safari endpoint sessions do not claim native semantic support; a
  native-only operation returns a typed `UnsupportedOperation` error.
- Update Rust SDK examples to exercise the canonical native observation path.

## Path

- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `crates/glass-browser/src/lib.rs`
- `docs/rust-sdk.md`
- `docs/features.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-729.md`
- `CHANGELOG.md`

## Verification

- Passed `cargo check --locked --quiet -p glass-browser --all-targets`.
- Passed `cargo test --locked --quiet -p glass-browser --lib
  standard_semantic_observation_rejects_non_native_backend_typed` (1 test).
- Passed the focused integration tests
  `native_semantic_levels_and_region_expansion_are_revision_scoped` and
  `native_runtime_exposes_shared_semantic_session_contracts` (1 test each).
- The first integration test starts via `BrowserSession`, observes a button,
  clicks its semantic reference under the observation revision, and verifies
  stale region expansion is rejected. The unit test checks typed rejection for
  all five canonical observation methods on a non-native backend.
- Passed `cargo fmt --all -- --check`, `git diff --check`, release-truth
  (1,357 Markdown documents; 0 current-claim failures), documentation-depth
  (93 current guides; 19 substantive contracts), and shortcut inventory
  (15 implementation keys; 63 documentation markers).
- Remote CI and live CLI documentation inventory were not run. Issue #40
  remains open; this slice does not certify Core Web Profile parity.

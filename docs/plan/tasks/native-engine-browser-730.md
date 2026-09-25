---
id: native-engine-browser-730
scope: glass-browser/public-rust-topology
status: done
depends-on: [native-engine-browser-729]
---

# Glass native-engine browser slice 730: canonical topology and frame text

## Objective

Expose the existing native page-target and frame registry through standard
`BrowserSession` methods so Rust callers can manage browsing contexts without
using native-internal method names or CDP. Ensure parent visible-text
observations do not include HTML iframe/frame fallback descendants when the
native runtime owns those browsing contexts.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/architecture/browser.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/rust-sdk.md`
- `docs/plan/tasks/native-engine-browser-729.md`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/session/target.rs`
- `crates/glass-browser/src/browser/session/frame.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- `BrowserSession::list_targets`, `create_target`, `select_target`,
  `close_target`, `list_frames`, and `select_frame` use the existing native
  target/frame registry and preserve its selection, opener, and lifecycle
  semantics.
- Target creation remains parked and does not change the active target.
  Selection is explicit; closing the active target does not silently choose a
  replacement.
- A non-native endpoint receives a typed
  `BrowserBackendError::UnsupportedOperation` before native state access.
  These methods never probe, launch, or fall back to CDP.
- HTML `iframe` and `frame` fallback content remains represented in the DOM
  but is excluded from the parent document's visible-text projection; the child
  frame is inspected through its own selected frame context. The projection
  does not rewrite the underlying fallback content.
- Existing `native_*` spellings remain source-compatible. This slice does not
  change the native topology owner or claim complete browsing-context parity.

## Path

- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-730.md`
- `docs/features.md`
- `docs/rust-sdk.md`
- `CHANGELOG.md`

## Verification

- Exercise target creation/listing/selection/closure through `BrowserSession`
  and verify a newly created page remains parked until selected.
- Exercise frame listing and explicit frame selection through `BrowserSession`
  using the existing native child-frame fixture.
- Verify parent visible text excludes iframe fallback text without removing
  or rewriting fallback content in the underlying DOM.
- Verify every new method rejects a non-native backend with a typed error and
  does not switch transports.
- After the coherent code/docs batch, run one locked package-scoped browser
  all-target check, then only the focused topology integration tests and typed
  error unit test. Run formatting, whitespace, release-truth,
  documentation-depth, and shortcut gates. Do not run workspace-wide tests,
  remote CI, `cargo clean`, or mark issue #40 complete.

### Results

- `cargo check --locked --quiet -p glass-browser --all-targets` passed.
- `standard_topology_rejects_non_native_backend_typed` passed (1 test),
  covering all six new methods.
- `native_runtime_session_preserves_independent_target_state_and_lifecycle`
  and `native_runtime_session_owns_and_routes_child_frames` passed (1 test
  each) through canonical `BrowserSession` startup.
- `visible_text_excludes_html_iframe_fallback_descendants` passed (1 test),
  asserting that parent visible text omits fallback while raw fallback markup
  remains unchanged.
- The first child-frame run exposed the fallback-text projection defect; the
  final focused run passed after the fix. Remote CI was not run. Issue #40
  remains open.
- `cargo fmt --all -- --check` and `git diff --check` passed. Release-truth
  validated 1,358 Markdown documents with zero current-claim failures;
  documentation-depth validated 93 current guides and 19 substantive
  contracts; shortcut inventory validated 15 implementation keys and 63
  documentation markers.
- The live documentation-coverage inventory was skipped because
  `target/debug/glass` is absent. Remote CI and platform certification were not
  run; issue #40 remains open.

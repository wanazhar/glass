id: native-engine-browser-732
scope: glass-browser/native-navigation-cancellation
status: done
depends-on: [native-engine-browser-731]
---

# Glass native-engine browser slice 732: real stop-loading cancellation

## Objective

Make revision-guarded stop-loading cancel an active native HTTP(S) navigation
through the canonical Rust `BrowserSession` and the in-process native browser
TUI. Cancellation must preserve the committed document and history, and leave
the content-worker boundary safe for the next operation.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/architecture/tui.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/rust-sdk.md`
- `docs/plan/tasks/native-engine-browser-731.md`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/tui/app.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- Native `BrowserSession::stop_loading_with_revision(revision)` requests
  cancellation of an in-flight process-backed HTTP(S) navigation only when
  `revision` matches the active navigation's starting revision. It returns
  promptly when accepted; callers must continue polling their navigation
  future until that operation settles. An idle stop validates the current
  revision and returns the unchanged revision. If commit has already won the
  race, stop returns a typed lifecycle error instead of claiming cancellation.
- Cancelling an in-flight content-worker load terminates and reaps that worker
  before the navigation future returns. The cancelled document is not
  committed, history and revision remain unchanged, and request-ledger
  accounting is balanced. The next navigation starts a fresh worker. Aborting
  discards transient state held only by that content worker; the committed
  parent-owned document remains intact.
- A stale revision is rejected without cancelling a newer navigation. Firefox
  and Safari return typed unsupported-operation errors; there is no CDP or
  other-runtime fallback.
- The direct, in-process native TUI polls navigation alongside input and routes
  `Alt+S` Stop-loading through the canonical session method. At this slice
  checkpoint, the separate persistent-owner socket still processed requests
  serially and reported stop-loading as unsupported; slice 733 closes that
  gap.
- This slice does not claim complete network-stream cancellation, cancellation
  of every navigation source, persistent-owner interruption, or profile
  conformance.

## Path

- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/error.rs`
- `crates/glass-browser/src/browser/native_engine/cancellation.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/tui/app.rs`
- `crates/glass-browser/src/browser/persistent.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/architecture/tui.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-732.md`
- `docs/features.md`
- `docs/rust-sdk.md`
- `CHANGELOG.md`

## Verification

- Use a delayed loopback HTTP response to prove that a current-revision stop
  request returns promptly, then the navigation future settles after polling,
  preserves the prior URL/history/revision, and leaves no child worker behind.
- Prove a subsequent native navigation succeeds after cancellation, proving
  worker recovery rather than merely checking the cancellation error.
- Cover idle stop and stale-revision rejection in the focused native integration
  test; exercise `Alt+S` against a delayed loopback response in the direct TUI.
- The persistent-owner stop response was explicitly unsupported at this
  checkpoint. Its assertion compiled under the all-target check, but the
  library unit-test harness was not run in this slice; slice 733 implements and
  exercises the concurrent control path.
- After the coherent code/docs batch, run one locked package-scoped
  `glass-browser` all-target check, then focused cancellation/control tests.
  Run formatting, whitespace, release-truth, documentation-depth, and shortcut
  gates. Do not run workspace-wide tests, remote CI, `cargo clean`, or mark
  issue #40 complete.

## Results

- `cargo check --locked --quiet -p glass-browser --all-targets` passed.
- After adding the same-document cancellation/commit boundary, the final `cargo check --locked --quiet -p glass-browser --lib` passed.
- `cargo test --locked -p glass-browser --test native_engine canonical_browser_session_stop_loading_cancels_and_restarts_worker -- --nocapture` passed (1 test).
- The test verifies idle stop, stale-revision rejection, prompt cancellation acceptance, unchanged committed URL/revision, content-worker reaping, and successful navigation after worker restart.
- `cargo build --locked --quiet -p glass-browser --bin glass-browser` passed. A PTY smoke with a delayed loopback server showed the native TUI accepting `Alt+S`, reporting `Navigation stopped · the pending document was not committed`, and exiting cleanly.
- Release-documentation truth passed for 1,360 Markdown documents with zero current-claim failures; documentation depth passed (93 guides, 19 contracts); TUI shortcut inventory passed (15 implementation keys, 63 documentation markers); `git diff --check` passed.
- At this checkpoint, persistent-owner stop still returned an explicit unsupported response; slice 733 implements concurrent controls.
- The package-wide test suite, persistent-owner unit test, and remote CI were not run; no full-workspace test or clean was performed.

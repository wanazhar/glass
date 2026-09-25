id: native-engine-browser-733
scope: glass-browser/persistent-native-owner-controls
status: done
depends-on: [native-engine-browser-732]
---

# Glass native-engine browser slice 733: persistent-owner stop-loading

## Objective

Make a native persistent owner accept revision-checked Stop Loading while it is
servicing a browser operation, and wire the attached native TUI `Alt+S` control
to it. Keep one command as the browser-state writer: the owner must keep
polling the original command while accepting status and stop requests, reject
other concurrent commands explicitly as busy, and reject stop promptly when
no process-backed HTTP(S) navigation can be interrupted. The original command
must settle cancellation and reap its content worker before the owner handles
the next browser operation.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/architecture/tui.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-732.md`
- `crates/glass-browser/src/browser/persistent.rs`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/tui/app.rs`

## Contract

- A native owner still has a single active browser operation and does not
  dispatch a second state-changing operation concurrently.
- While that operation is pending, the Unix socket loop continues polling it
  and accepts `status` plus revision-checked `control(stopLoading)` requests.
- The owner services `stopLoading` only when the native backend reports an
  active process-backed HTTP(S) navigation. It invokes the canonical
  `BrowserRuntimeSession::stop_loading_with_revision`; stale revisions and a
  navigation that already claimed commit return explicit errors. A stop during
  other active work fails promptly and does not wait on the operation lock.
- The stop response acknowledges the cancellation request; the original
  command request continues until the content worker is terminated/reaped and
  its partial response discarded. Only then may another browser command run.
- While one operation is active, all other mutating/browser-operation requests
  receive an explicit busy response instead of queueing, reordering, or running
  against the same state concurrently.
- With no active operation, existing persistent controls continue to work;
  idle stop validates the revision and returns the unchanged revision.
- Attached native TUI `Alt+S` sends the persistent owner control and reports
  its result. No transport fallback or false-success no-op is allowed.
- The native owner remains local Unix-socket based. This slice does not claim
  interruption of non-HTTP navigation sources, full request-stream parity,
  Windows persistent-owner support, or browser-profile completion.

## Path

- `crates/glass-browser/src/browser/persistent.rs`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/tui/app.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/architecture/tui.md`
- `docs/plan/README.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-733.md`
- `CHANGELOG.md`

## Verification

- In a process-backed native persistent-owner test, start a delayed
  loopback HTTP navigation and wait until the fixture receives it. While that
  owner command is pending, prove `status` still responds, an unrelated
  concurrent browser command is rejected as busy, a stale stop is rejected,
  and a current-revision stop returns promptly.
- Prove the original command settles as cancelled, the committed URL/revision
  remains unchanged, and a subsequent HTTP navigation through the same owner
  succeeds after a fresh content worker starts.
- Exercise idle stop and the attached persistent TUI `Alt+S` dispatch path.
- First run one locked, package-scoped `glass-browser` all-target check, then
  the focused persistent-owner cancellation test and directly relevant tests.
  Run formatting, whitespace, release-truth, documentation-depth, and shortcut
  gates after the coherent code/docs batch.
- Do not run workspace-wide tests, remote CI, `cargo clean`, or claim issue #40
  completion.

## Results

- `cargo check --locked --quiet -p glass-browser --all-targets` passed after
  the owner-control and optional deep-DOM completion-field changes.
- `cargo test --locked --quiet -p glass-browser --lib native_owner_keeps_one_engine_alive_for_multiple_ipc_commands -- --nocapture`
  passed (1 test): idle stop, active status, busy rejection, stale revision,
  prompt accepted cancellation, unchanged committed revision, and a successful
  second HTTP navigation through the same owner.
- `cargo build --locked --quiet -p glass-browser --bin glass-browser` passed.
  PTY smoke attached the TUI to a native persistent owner, navigated to
  `about:blank`, decoded the deep-DOM projection without `complete`, and sent
  `Alt+S`; the owner response was shown as `stopLoading requested`.
- Rust formatting and `git diff --check` passed. Release-truth validated 1,361
  Markdown documents with zero current-claim failures; documentation depth
  validated 93 guides/19 contracts; shortcut inventory validated 15 keys and
  63 markers.
- The live documentation-coverage command was attempted but requires
  `target/debug/glass`; it was not built because that binary belongs to the
  unaffected `glass-dev` package. Remote CI was not run. No workspace test or
  `cargo clean` was run; 82 GB remained free.

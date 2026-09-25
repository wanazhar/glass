---
id: native-engine-browser-742
scope: glass-browser/synchronous-native-history-lifecycle
status: done
depends-on: [native-engine-browser-741]
---

# Glass native-engine browser slice 742: synchronous history lifecycle

## Objective

Close the public synchronous `NativeEngine::go_back()` and `go_forward()`
cross-document lifecycle bypass without blocking on asynchronous dialog or
content-process control. Preserve the canonical async `BrowserSession` route
as the path for process-backed history and modal decisions.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-741.md`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/history.rs`

## Contract

- Same-document Back/Forward traversal remains synchronous and keeps its
  current no-reload lifecycle.
- For a local cross-document target, dispatch `beforeunload` before loading or
  preparing the target. If the event is canceled and the outgoing document
  has sticky activation, return an explicit error directing the caller to the
  asynchronous API. Preserve the active document/history entry, retain
  `beforeunload` callback mutations, and do not dispatch `pagehide`/`unload` or
  load the target.
- If a local `beforeunload` event is not canceled, or is canceled without
  sticky activation, continue with the current lifecycle and commit the target
  exactly once. A native host call that re-enters cross-document history while
  outgoing lifecycle dispatch is active returns a typed error instead of
  recursing or partially committing. A cross-document `HistoryGo` surfaced
  inline from a process-backed outgoing lifecycle callback is rejected before
  nested traversal or history-selection mutation. Page-initiated History API
  commands outside outgoing lifecycle dispatch keep their queued semantics.
- A process-backed outgoing document cannot be synchronously queried for
  lifecycle events or modal decisions. Return a typed error before issuing a
  child command, loading a target, or mutating the history selection. Direct
  callers must use `go_back_async()`/`go_forward_async()` or `BrowserSession`.
- Do not add CDP fallback, silently skip lifecycle events, or weaken the
  asynchronous production path.

## Tradeoffs

The synchronous API cannot present a responsive user decision. Activated
canceled `beforeunload` therefore fails closed, while process-backed
cross-document traversal requires the async API even when no modal is known to
be pending. The re-entry guard covers native host-level recursion and inline
process-backed cross-document `HistoryGo` commands during outgoing lifecycle
dispatch; ordinary page History API commands retain queued semantics outside
that callback boundary. Async lifecycle context is passed explicitly through
mutation handling instead of being stored in a mutable counter across `.await`,
so dropping the future cannot leave a stale re-entry guard set.

## Path

- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-742.md`

## Verification

- Test activated, canceled local `beforeunload`: typed error; active URL,
  history selection, and outgoing JavaScript callback state remain unchanged;
  no `pagehide`, `unload`, or target load occurs. Global JavaScript state is
  not asserted to advance the semantic document revision.
- Test local traversal without sticky activation: the lifecycle runs once and
  the intended history target commits.
- Test sync and async native-host re-entry while outgoing lifecycle dispatch is
  active: both return a typed error without changing URL/history selection.
- Test process-backed synchronous traversal: typed error before history
  activation or another target request; confirm the async `BrowserSession`
  path remains usable.
- Test process-backed `beforeunload` issuing `history.back()`: the async
  traversal returns the typed re-entry error, retains the current URL/history
  selection and callback mutation, and does not dispatch `pagehide`/`unload` or
  request another target.
- Test same-document Back/Forward remains in-place and existing async
  beforeunload history accept/dismiss tests remain green.
- Run an affected-target `cargo check` before focused tests; run formatting,
  documentation truth/depth/shortcut/coverage gates, and `git diff --check`.
- Report Linux-local, remote CI, and cross-platform evidence separately.

## Local evidence

2026-09-25 Linux-local checks:

- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- `cargo test -p glass-browser --lib synchronous --locked --quiet` passed five
  focused tests for sync fail-closed behavior, no-activation lifecycle order,
  same-document Back/Forward, and sync/async host re-entry.
- `cargo test -p glass-browser --test native_engine beforeunload --locked --quiet`
  passed five host/history tests, including the process-backed synchronous API
  rejection, inline `history.back()` re-entry rejection from process-backed
  `beforeunload`, and the existing async Back/Forward confirmation route.
- `rustfmt --edition 2024` on the two changed Rust files and `git diff --check`
  passed.
- Documentation truth, depth, shortcut, and coverage gates passed after the
  final documentation update.

These results are Linux-local only. They do not certify remote CI, Windows,
macOS, descendant-frame lifecycle, or sandbox-modal propagation. Issue #40
remains open.

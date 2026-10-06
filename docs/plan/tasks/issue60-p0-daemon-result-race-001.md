---
id: issue60-p0-daemon-result-race-001
scope: glass-dev/daemon-operation-completion
status: ready
depends-on: []
---

# Issue #60 P0: preserve completed daemon results across cancellation races

## Objective

Resolve the standalone daemon finding from source report #55: a completed
operation result must not be discarded solely because cancellation was also
requested. Record exactly one terminal state that reflects whether execution
completed or was actually cancelled.

## Context

- `crates/glass-dev/src/daemon.rs`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), daemon P0 data
  integrity finding
- [Source report #55](https://github.com/wanazhar/glass/issues/55)

## Contract

- If the operation has produced a completed result before cancellation takes
  effect, retain and return that result with a successful terminal state.
- Preserve ordinary worker errors as `Failed` with their failure reason, even
  when cancellation was requested while the operation was running.
- Report `Cancelled` when cancellation prevents a queued operation from
  starting. Running cancellation is intent only; it does not convert an
  ordinary worker error into cancellation.
- Completion, cancellation, polling, and repeated finish attempts must not
  produce conflicting terminal states or lose the result.

## Path

- Daemon operation state and `finish()` in `crates/glass-dev/src/daemon.rs`
- Running cancellation contract in `docs/daemon.md`
- Existing daemon operation unit tests and focused race regression coverage

## Verification

- Add deterministic coverage for cancellation requested after a result is
  available, cancellation requested while running before a successful result,
  cancellation requested while running before a worker error, cancellation
  winning before start, and repeated finish calls.
- Verify the public operation status/poll response retains the completed result
  in the completion-wins case.

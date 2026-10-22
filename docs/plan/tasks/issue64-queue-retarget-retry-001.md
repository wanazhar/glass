---
id: issue64-queue-retarget-retry-001
scope: issue-64-follow-up-queue-delivery
status: in-progress
depends-on: [issue64-live-run-queue-targeting-001, issue64-send-attempt-recovery-001]
---

# Follow-up queue retarget and retry journey

## Objective

Verify the user-visible follow-up queue recovery path: reject dispatch to an
older RunId until the user explicitly retargets the row, and require a
cancellable duplicate-risk confirmation before retrying a `NeedsReview` row.
The confirmed retry must reach the current run exactly once and settle only its
matching send attempt.

## Context

- [Glass documentation index](../../INDEX.md)
- [Development TUI architecture](../../architecture/development-tui.md)
- [Durable per-thread follow-up queue](issue63-durable-followup-queue-001.md)
- [Durable send attempt recovery](issue64-send-attempt-recovery-001.md)
- [Live-run queue targeting](issue64-live-run-queue-targeting-001.md)
- [Glass delivery plan](../README.md)

## Paths

- `crates/glass-dev/tests/tui_pty.rs`
- `docs/plan/README.md`

## Acceptance

- [x] A real PTY session with the offline fake Core fixture starts a resident
      thread whose registry has an old completed run and a current run.
- [x] Enter on a ready queue item targeting the old RunId leaves the target,
      state, attempt ledger, and Core request log unchanged; `t` explicitly
      retargets the item to the current RunId and persists it as ready.
- [x] A `NeedsReview` item targeting the current RunId opens its duplicate-risk
      confirmation. Escape cancels it without a new attempt or Core request.
- [x] A later explicit confirmation sends the item once, and the accepted
      queue row plus settled attempt retain the exact current target RunId.
- [ ] The focused PTY test, strict scoped Clippy, formatting, fixture syntax,
      documentation coverage/depth/release-truth/TUI-shortcut checks, and
      independent review pass. Evidence identifies the exact binary and
      distinguishes it from a fresh release build.

## Verification

Run the allocated journey with `GLASS_E2E=1` and an explicit
`GLASS_E2E_BINARY`. Run `cargo clippy -p glass-dev --test tui_pty --locked --
-D warnings`, `cargo fmt --all -- --check`, Python fixture syntax validation,
documentation coverage/depth/release-truth/TUI-shortcut checks, and
`git diff --check`.

## Evidence

The focused fake-Core PTY journey passed against the existing local debug
binary:

```text
CARGO_TARGET_DIR=/home/ubuntu/work/glass/target GLASS_E2E=1 \
  GLASS_E2E_BINARY=/home/ubuntu/work/glass/target/debug/glass \
  cargo test -p glass-dev --test tui_pty \
  follow_up_queue_old_run_retarget_and_review_retry_are_explicit \
  --locked -- --nocapture
1 passed; 0 failed; 27 filtered out
```

The PTY test first blocks a `ready` item whose destination is an older
completed RunId; neither its row nor the pre-existing uncertain attempt changes,
and the fake Core request log has no matching prompt. Pressing `t` explicitly
retargets that row and persists it as `ready` on the current RunId. For a
separate `NeedsReview` row, the test opens the duplicate-risk dialog, presses
Escape, and exercises queue selection again to confirm the dialog closed. The
original uncertain attempt and review state remain intact, with no Core request
or new attempt. A later Enter confirmation produces one fake-Core request and a
new settled attempt targeting the current RunId. The retry keeps the same client
message ID and gets a distinct attempt ID; the earlier uncertain record remains
uncertain. The queue row retains `followUp` mode; because the current run is
idle, Glass correctly routes it through Core's `turn/prompt` operation.

The test compiled from this branch and used
`/home/ubuntu/work/glass/target/debug/glass` (`glass 0.3.14`), an existing local
debug binary. No fresh Glass or release binary was built for this slice.

Validation passed: strict `cargo clippy -p glass-dev --test tui_pty --locked --
-D warnings`, `cargo fmt --all -- --check`, Python fixture syntax validation,
documentation coverage (1809 Markdown files), documentation depth, release
truth, TUI shortcut inventory, and `git diff --check`. Independent review is
pending re-review after remediation. Its first pass identified that the stale
dispatch assertion did not explicitly check that the row remained `ready`
before retargeting; the PTY test now asserts the persisted state at that point.

# Issue #60 P1 browser-crash recovery review 02

## Scope

Reviewed follow-up commit `2496ee32cb52fab91dcd8653afc51fb6e517fdd0`
against parent `f324d79617822db1784fabaf7155fe3c8aebe0cc`. Rechecked the
blocking finding from review 01, the F19 task contract, recovery status and
frame ownership, delayed Herdr events, and queued navigation retention and
dismissal. No implementation files were changed.

## Findings

- **P1/P2 blocking: none.** `offer_browser_recovery` now clears
  `frame_revision` and the cached browser pane after disconnect
  (`crates/glass-dev/src/tui/state.rs:9801-9807`). `Connected` requires both no
  pending recovery and a present frame revision matching the browser revision
  (`crates/glass-dev/src/tui/mod.rs:1467-1480`). The delayed-connection test
  starts with a matching Herdr frame, offers crash recovery, then verifies the
  frame is invalidated and the event cannot replace recovery status or
  presentation (`mod.rs:1934-1971`). This closes review 01's stale-frame/status
  blocker.
- **P3 non-blocking: combined event coverage could be sharper.** The recovery
  test verifies a delayed Failed event disables Herdr and reconciles to
  Semantic-only while retaining recovery status. It then sends Stopped after
  Failed has already disabled the runtime, so the Stopped reconciliation arm
  does not run in that test. The separate existing stopped-worker test covers
  that arm without recovery active. The implementation calls the same
  disable/reconcile sequence before restoring recovery status
  (`mod.rs:1490-1500`); a standalone Stopped-with-recovery assertion would make
  that combined behavior explicit. Also, the delayed Connected test delivers
  the event while recovery is pending; delivering it after dismissal would
  independently exercise the cleared-frame check after the pending-offer guard
  is gone. Both are coverage improvements, not observed behavior defects.

Queued navigation is now covered: the crash-snapshot regression seeds a URL,
checks it remains queued when recovery is offered, and checks dismissal clears
it without changing the selected surface (`crates/glass-dev/src/tui/state.rs:11057-11108`).

## Verification

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked delayed_herdr` — passed, 3 tests.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked crashed_snapshot_preserves_surfaces_and_defers_recovery_behind_active_ui` — passed, 1 test.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked browser_recovery` — passed, 5 tests.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked browser_tool_failure_preserves_picker_and_overlay_state` — passed, 1 test.
- `git diff --check 2496ee32^ 2496ee32` — passed.

## Conclusion

**PASS.** Review 01's blocking stale Herdr status issue is closed, and queued
URL preservation/dismissal now has regression coverage. The P3 notes above are
optional improvements; no blocking defect remains.

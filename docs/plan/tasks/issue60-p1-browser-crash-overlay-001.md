---
id: issue60-p1-browser-crash-overlay-001
scope: glass-dev/tui-browser-crash-recovery-overlay
status: complete
depends-on: [issue60-p1-tui-overlay-priority-001]
---

# Issue #60 P1: preserve the active surface during browser recovery

## Objective

Resolve F19 by keeping the selected TUI surface and active UI state intact when
a browser crash snapshot or qualifying browser tool failure offers recovery.
The recovery overlay must wait below every already-active overlay and
dismissal must return to the original surface.

## Context

- [Issue #60](https://github.com/wanazhar/glass/issues/60), finding F19
- [Source report #45](https://github.com/wanazhar/glass/issues/45), section 19
- [Development TUI architecture](../../architecture/development-tui.md)
- [Issue #60 delivery analysis](../analysis/issue-60.md)
- [Overlay-priority task](issue60-p1-tui-overlay-priority-001.md)

## Contract

- A crashed browser snapshot and a qualifying browser tool failure install an
  explicit recovery offer and status without switching the current surface or
  closing a picker or other overlay.
- Browser recovery has lower priority than every other overlay. Help, Quit,
  editor exit, full-screen editor, pickers, approval/confirmation, composer,
  and palette state remain authoritative while active.
- Once higher-priority UI closes, the pending recovery offer becomes visible.
  Dismissing it clears the offer and pending navigation without changing the
  surface that was active before recovery was offered.
- Recovery invalidates the last browser frame revision. Delayed Herdr
  connection/failure events cannot replace the recovery status; failure still
  disables and reconciles the visual runtime.
- Trust, Code/full-screen editor, Help, Quit, and a regular surface receive
  focused regression coverage where practical.

## Path

- `crates/glass-dev/src/tui/overlay.rs`
- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `docs/architecture/development-tui.md`
- `docs/plan/README.md`
- `docs/plan/analysis/issue-60.md`
- `docs/plan/tasks/issue60-p1-browser-crash-overlay-001.md`

## Verification

- Exercise recovery overlay priority against Help, Quit, editor exit, full-screen
  editor, pickers, and other active overlays.
- Apply a crashed-browser snapshot on Trust, Code/full-screen editor, Help,
  Quit, and a regular surface; verify surface preservation, explicit recovery
  status, and dismissal behavior.
- Exercise a qualifying browser tool failure with existing picker/overlay state
  and verify recovery does not close or replace it.
- Run focused state/overlay tests, `cargo check -p glass-dev --lib --bins
  --locked`, formatting, diff, and relevant documentation checks. Record any
  unrelated documentation coverage drift.
- Obtain independent review before integration.

## Validation evidence

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev
  --lib --locked delayed_herdr`: passed, 3 tests, including delayed Connected
  and Failed events while App + Composer and browser recovery are active. Failed
  still disables Herdr and reconciles presentation to Semantic-only; Stopped
  also preserves recovery status.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev
  --lib --locked browser_recovery`: passed, 5 tests, including the recovery
  precedence regression across all active overlay kinds.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev
  --lib --locked
  crashed_snapshot_preserves_surfaces_and_defers_recovery_behind_active_ui`:
  passed, 1 test covering Trust, Code/full-screen editor, Help, Quit, and a
  regular Terminal surface, including queued-navigation retention on offer and
  clearing on dismissal.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev
  --lib --locked browser_tool_failure_preserves_picker_and_overlay_state`:
  passed, 1 test covering browser failure with Help and the target picker active.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-dev
  --lib --bins --locked`: passed. It emitted the existing 72 dead-code warnings
  from `glass-browser`'s native engine; no new `glass-dev` warning was reported.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `python3 scripts/check-release-documentation.py --require-previous-version`:
  passed (1,558 Markdown documents audited; 0 current-claim failures).
- `python3 scripts/check-documentation-depth.py`: passed (93 current guides,
  19 substantive contracts).
- `python3 scripts/check-tui-shortcuts.py`: passed (15 implementation keys,
  63 documentation markers).
- `python3 scripts/check-documentation-coverage.py --glass
  /home/ubuntu/work/glass/target/debug/glass --glass-browser
  /home/ubuntu/work/glass/target/debug/glass-browser`: blocked by the existing
  development MCP conformance fixture mismatch and missing live measurements
  in `docs/mcp-schema-budget.md` (`177` negotiated tools and a `76,967`-byte
  serialized `tools` array). The browser recovery task does not change MCP
  inventory or schema data.
- Independent review 01 (report commit `3219ca2d`) found a blocking
  delayed-Herdr status overwrite and a non-blocking queued URL assertion gap.
  The follow-up invalidates stale frame ownership, keeps Connected events from
  promoting a pending recovery, preserves recovery status after Herdr
  failure/stop while still reconciling the runtime, and covers queued URL
  retention and dismissal clearing.
- Independent [review 01](../reviews/issue60-p1-browser-crash-overlay-001-01.md)
  recorded the initial P2 blocker; follow-up [review 02](../reviews/issue60-p1-browser-crash-overlay-001-02.md)
  passed at implementation commit `2496ee32` with no blocking findings.

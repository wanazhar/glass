---
id: issue60-p1-tui-visual-state-001
scope: glass-dev/tui-browser-visual-lifecycle
status: in-progress
depends-on: [issue60-p1-tui-overlay-priority-001, issue60-p1-worker-request-coalescing-001]
---

# Issue #60 P1: pause live browser visuals when the pane is hidden

## Objective

Resolve F9's live visual lifecycle defect. Schedule screenshots only while the
rendered App visual pane is available, reject captures that became hidden
in-flight, and mark presentation paused/resuming across hide and show while
keeping the live-view toggle enabled.

## Context

- [Issue #60](https://github.com/wanazhar/glass/issues/60), finding F9
- [Source report #42](https://github.com/wanazhar/glass/issues/42), section 9
- [Development TUI architecture](../../architecture/development-tui.md)
- [Overlay-priority task](issue60-p1-tui-overlay-priority-001.md)
- [Worker request-coalescing task](issue60-p1-worker-request-coalescing-001.md)

## Finding and contract

The source report's pair-apply mode-label claim does not reproduce on the
current branch: the Factory Home header displays the selected editor mode
through its label. The live capture lifecycle does reproduce. Periodic
screenshots are submitted while the rendered visual area is absent, and
ANSI/Herdr results can overwrite presentation state after an occluding overlay
or a non-App surface hides that pane. Kitty clears its terminal pixels on hide,
but its workspace presentation and frame revision remain advertised as fresh.

- Capture only when the same rendered App visual area used by the UI exists.
  App and the non-occluding Composer overlay keep capture available; an
  occluding overlay, a non-App surface, or an empty rendered pane pauses it.
- Size captures from the rendered pane bounds.
- Discard a result if its capture was outstanding when the pane became hidden,
  even if the pane is visible again when the result arrives. Also reject results
  when the current pane is unavailable or the visual runtime is no longer live.
- On hide, clear the cached ANSI pane and current frame revision, mark the
  presentation/status paused, and preserve the live toggle. On return, show a
  resuming state until a new visible frame is accepted.
- Keep the selected backend in the presentation-path field while paused. The
  paused/resuming reason, absent frame revision, and cleared ANSI pane describe
  freshness; retaining the backend lets live presentation resume without
  changing browser settings.
- Keep browser semantic inspection, browser live-view settings, and standalone
  Browser TUI behavior unchanged.

## Path

- crates/glass-dev/src/tui/mod.rs
- crates/glass-dev/src/tui/snapshot.rs
- crates/glass-dev/src/tui/render.rs
- docs/architecture/development-tui.md
- docs/plan/README.md
- docs/plan/analysis/issue-60.md

## Verification

- Verify capture availability for visible App, App with Composer, an occluding
  overlay, and a non-App surface.
- Verify hide clears the current visual state and retains the live toggle;
  returning to App displays a resuming state.
- Verify a result captured before hide is discarded after resume and a fresh
  visible capture clears the pause state.
- Run focused TUI visual tests, the glass-dev package check, formatting, and
  documentation checks. Record any unrelated documentation-coverage drift.
- Obtain an independent review before merge.

## Validation evidence

- `cargo test -p glass-dev --lib --locked visual_`: passed, 9 tests.
- `cargo test -p glass-dev --lib --locked
  paused_visual_reason_overrides_selected_backend_active_placeholder --
  --nocapture`: passed, 1 test.
- `cargo check -p glass-dev --lib --bins --locked`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `python3 scripts/check-documentation-depth.py`: passed (93 guides, 19
  substantive contracts).
- `python3 scripts/check-tui-shortcuts.py`: passed (15 implementation help
  keys, 63 documentation markers).
- `python3 scripts/check-release-documentation.py --require-previous-version`:
  passed (1,556 Markdown documents audited).
- `python3 scripts/check-documentation-coverage.py`: blocked by the existing
  MCP conformance fixture mismatch and missing live measurement in
  `docs/mcp-schema-budget.md`: 177 negotiated tools and a 76,967-byte
  serialized `tools` array. The new task and architecture links pass the
  script's repository-link validation.

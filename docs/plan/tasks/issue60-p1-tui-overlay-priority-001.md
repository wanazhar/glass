---
id: issue60-p1-tui-overlay-priority-001
scope: glass-dev/tui-overlay-routing
status: in-progress
depends-on: [issue60-p0-tui-safety-001]
---

# Issue #60 P1: centralize TUI overlay priority

## Objective

Resolve F1 by making the active overlay's priority authoritative for keyboard,
paste, mouse, pointer hit-testing, Ratatui overlay composition, and terminal
redraw masking. Prevent input from reaching a covered surface or lower-priority
overlay. Include both file and session pickers in redraw transitions.

## Context

- `docs/architecture/development-tui.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), finding F1
- [Source report #42](https://github.com/wanazhar/glass/issues/42)

## Contract

- One typed resolver returns the top active overlay in the precedence order
  documented by `docs/architecture/development-tui.md`.
- The event loop dispatches keyboard and paste input to only that overlay;
  otherwise it uses ordinary surface input. Ctrl-C keeps its documented global
  quit behavior after the editor exit prompt.
- Mouse events route only to the top overlay. The command-center menu remains
  pointer-selectable, help wheel input scrolls help, and other overlays block
  clicks and wheel input from reaching the covered surface.
- Pointer hit-testing and rendering select the same overlay as keyboard
  dispatch. File/session pickers cannot be hidden by lower-priority editor or
  Pi overlays.
- Terminal redraw masking is derived from the same top-overlay resolver and
  changes when file/session picker overlays open or close. Git diff's existing
  terminal presentation bit remains represented.
- Overlay transitions reset in-progress pointer gestures so a press begun on a
  covered surface cannot trigger an action after a modal opens.

## Path

- `crates/glass-dev/src/tui/overlay.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `crates/glass-dev/src/tui/pointer.rs`
- `crates/glass-dev/src/tui/render.rs`
- `docs/architecture/development-tui.md`
- `docs/plan/README.md`
- `docs/plan/analysis/issue-60.md`

## Verification

- Cover contradictory overlay flags and verify the resolver returns the
  documented highest-priority overlay, including file/session pickers over
  editor/Pi, menu/help ordering, and composer/editor focus.
- Exercise keyboard and paste routing while a second overlay is active; verify
  only the top overlay changes and covered surface state remains unchanged.
- Exercise pointer click, wheel, drag, and long-press paths under each modal;
  verify covered selections and actions remain unchanged and menu/help retain
  their supported pointer behavior.
- Verify rendered overlay selection and terminal redraw mask use the resolver,
  including file/session picker open/close transitions.
- Run focused TUI tests, package checks, formatting, and the independent review.

## Validation evidence

- `cargo test -p glass-dev --lib --locked overlay`: passed, 10 tests.
- `cargo test -p glass-dev --lib --locked picker`: passed, 11 tests.
- `cargo test -p glass-dev --lib --locked help_rendering`: passed, 1 test.
- `cargo check -p glass-dev --lib --bins --locked`: passed.
- `cargo build -p glass-dev --bin glass --locked` and
  `cargo build -p glass-dev --bin glass-browser --locked`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `check-release-documentation.py --require-previous-version`,
  `check-documentation-depth.py`, and `check-tui-shortcuts.py`: passed.
- `check-documentation-coverage.py`: ran after building its binaries, but is
  blocked by the current development MCP conformance fixture mismatch and
  missing live measurements in `docs/mcp-schema-budget.md` (177 negotiated
  tools; 76,967-byte serialized `tools` array). Those files are outside this
  task's scope and unchanged.
- Independent review pending; task status remains `in-progress`.

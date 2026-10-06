# Independent re-review: issue60-p1-tui-overlay-priority-001

Commit reviewed: `7fc269f11628593a188c868c8b27a59d148ddef6`

## Verdict: PASS

The command-menu Down→poll→Up path now works, and overlay transitions still
cancel gestures begun on the covered surface. I found no remaining blocking or
non-blocking pointer/render geometry findings in the reviewed F1 changes.

## Review evidence

- `crates/glass-dev/src/tui/pointer.rs:151-168` preserves a press only when the
  active overlay is the command menu and the press began with the left button
  on a rendered `HitRegion::Menu` row. Other active overlays still clear the
  pointer state. The Up handler only treats a long press as the global menu
  gesture when no overlay is active (`pointer.rs:129-143`), so a held menu-row
  press completes through that row's click action.
- The event loop resets pointer state on active-overlay changes before and
  after polling (`crates/glass-dev/src/tui/mod.rs:1077-1080`). A press begun on
  a surface is therefore discarded when an overlay opens.
- The rendered list and menu hit test share `command_menu_geometry`, including
  list bounds and scroll offset. The previous details-panel/outside-pane defect
  remains closed.
- Regression coverage exercises Down→poll→Up for a menu action and a covered
  surface press across an overlay open/close transition.

## Validation

- `cargo test -p glass-dev --lib --locked tui::pointer::tests` — passed, 10 tests.
- `cargo test -p glass-dev --lib --locked tui::tests::overlay_opening_discards_a_press_started_on_the_covered_surface` — passed, 1 test.
- `git diff 7fc269f1^ 7fc269f1 --check` — passed.

No implementation files were modified during this review.

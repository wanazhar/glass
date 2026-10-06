# Independent review: issue60-p1-tui-overlay-priority-001

Commit reviewed: `845798cb3a4e8ad2198834f34b9727e5ad3f6e61`

## Verdict: BLOCKED

The shared overlay resolver now drives keyboard and paste dispatch, render
selection, pointer modal blocking, and the terminal overlay mask. File and
session pickers outrank editor/Pi overlays; help outranks the command menu;
Composer preserves browser pixels while picker overlays clear them. The
focused TUI suite passes. One blocking P2 defect remains in command-menu pointer
hit-testing: it treats non-list coordinates as selectable menu rows, which can
run an action when the user clicks the details panel.

## Findings

### P2 — Command-menu hit-testing maps details and outside-pane clicks to actions (blocking)

In `crates/glass-dev/src/tui/pointer.rs:173-183`, the `CommandCenterMenu`
hit-test checks only the row and computes an index as `row - 3`; it ignores the
column and the actual visible list bounds. The menu is rendered only inside the
surface pane in `crates/glass-dev/src/tui/render.rs:1396-1409`, with its list in
`rows[0]` and a separate details panel in `rows[1]` at `render.rs:3750-3771`.
`apply_click` immediately assigns the returned index and calls
`run_menu_action()` at `pointer.rs:328-331`.

Repro on a phone terminal sized 48x18 with the Agent command menu open: the
surface pane starts at row 2 and is 13 rows tall, so the menu layout allocates
8 rows to the list and 5 rows to the details panel. Clicking the details panel
at row 10 yields `Menu(7)` from `row - 3`; the Agent menu has an item at index 7,
so the click runs “Inspect Pi session tree” instead of remaining inert. On
desktop, a click in the navigation or context columns at a menu-row y-coordinate
also maps to a menu item because `column` is unused.

Hit-testing needs to use the rendered menu/list rectangle and its current
visible item offset, returning `Other` outside actual list rows. Add pointer
regressions for the details panel and outside-pane coordinates, plus a scrolled
menu list.

## Validation

- `cargo test -p glass-dev --lib --locked tui::` — passed, 231 tests.
- `git diff 845798cb^ 845798cb --check` — passed.

No implementation files were modified during this review.

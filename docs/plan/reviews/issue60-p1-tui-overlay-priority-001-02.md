# Independent re-review: issue60-p1-tui-overlay-priority-001

Commit reviewed: `f4f425ac36ed2351c9d81bafa1e392145fd50fff`

## Verdict: BLOCKED

The prior menu geometry finding is fixed. Rendering and hit-testing now use
shared menu regions, list bounds, and the selected-row scroll offset; tests
cover detail-panel clicks, coordinates outside the pane, and scrolled rows.
However, the command menu still cannot receive a normal mouse click in the
interactive event loop because pointer polling clears the press before the
subsequent release event.

## Findings

### P2 — Active-overlay polling clears command-menu presses before release (blocking)

`PointerState::handle` stores a press on mouse Down and runs the menu item on
mouse Up (`crates/glass-dev/src/tui/pointer.rs:95-115, 125-143`). Between those
events, `PointerState::poll` resets all pointer state whenever any overlay is
active (`pointer.rs:151-155`). The TUI event loop calls `pointer.poll` after
every event, including immediately after handling a mouse event
(`crates/glass-dev/src/tui/mod.rs:1064-1067, 1077-1080`).

Repro: open the command-center menu, then click a visible menu row. The Down
event stores its `PointerDown`; the event-loop poll sees `CommandCenterMenu`
active and clears it; the Up event then takes no pending press and returns
without calling `apply_click`. This makes the menu non-selectable by mouse in
the running TUI, despite direct `PointerState::handle(Down)` / `handle(Up)` unit
tests passing. Those tests omit the intervening poll used by the event loop.

Keep overlay-transition cancellation, but preserve a press/release gesture
while the same interactive overlay remains active. Add a regression that
dispatches Down, calls `poll`, dispatches Up, and verifies the menu row action
runs; also verify a press begun before an overlay transition is still cleared.

## Resolved prior finding and geometry review

The previous review's menu hit-region defect is closed: hit-testing now uses
`command_menu_geometry_for_screen`, while rendering uses the same
`command_menu_geometry` helper for list/detail bounds and scroll offset. The
new tests reject detail and outside-pane coordinates and map visible rows to
their scrolled item indexes. No further render/hit geometry mismatch was found.

## Validation

- `cargo test -p glass-dev --lib --locked tui::pointer::tests` — passed, 9 tests.
- The new tests cover menu geometry but do not model the event-loop poll
  between pointer Down and Up described above.

No implementation files were modified during this review.

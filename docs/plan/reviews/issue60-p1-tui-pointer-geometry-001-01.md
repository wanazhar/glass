# Issue #60 P1 TUI pointer-geometry review

## Scope reviewed

Reviewed commit `d1829fc26626776a8e5ed4d11da0ee2891dad34e` against parent `789c8c80389dc85ad82895051f944c3dafea6401`. Read the task record, `docs/INDEX.md`, and the Development TUI contract. Audited shared shell/navigation/surface/list geometry, scroll and visible-row bounds, empty states, App inspector entities, responsive layouts, composer footer, command menu/More routes, and F1 overlay gating.

## Findings

### P2 · blocking — More-route hit testing uses a duplicate panel layout and accepts border columns

The task requires the rendered More-route geometry to be shared by rendering and pointer hit testing (`docs/plan/tasks/issue60-p1-tui-pointer-geometry-001.md:29-41`). The Development TUI contract also says panel borders do not produce item selections (`docs/architecture/development-tui.md:188-194`). However, `more_route_at` gets its rectangle from `more_route_panel_area` (`crates/glass-dev/src/tui/render.rs:3845-3911`), while `render_more_surface` lays out and renders the route panel separately (`render.rs:3914-4019`). It also checks the full outer panel x range (`render.rs:3900-3905`) instead of the padded content area used by `render_scrolled_panel`.

Repro: on the More surface, click the left border column `route_area.x` at a route text row (`route_area.y + 1` for the first row). `hit_test` returns `MoreRoute(0)` and pointer selection changes even though the click is on the panel border, outside the rendered route text. The duplicate layout calculations can also drift as rendering changes. Use one More-surface geometry value for both rendering and hit testing, then limit route hits to the rendered content rectangle.

### P2 · blocking — Short composer dock is one row taller in the full-screen editor than its hit region

The task requires every row inside the rendered composer dock to map to `Dock` (`docs/plan/tasks/issue60-p1-tui-pointer-geometry-001.md:58-60`); the Development TUI contract requires dock hits to follow its rendered dynamic footer (`docs/architecture/development-tui.md:188-194`). `footer_height` returns 3 for a one-line composer (`crates/glass-dev/src/tui/render.rs:127-132`), and `hit_test` uses `screen_geometry(...).footer` (`crates/glass-dev/src/tui/pointer.rs:188-201`). But when the full-screen editor remains visible behind Composer, `render_fullscreen_editor` allocates `footer_height(state).max(4)` (`render.rs:611-619,646-660`).

Repro: enter Code full-screen edit, open Composer with one input line, and click row `terminal_height - 4` within the rendered dock. The rendered dock begins there, but the hit-test footer begins at `terminal_height - 3`, so `hit_test` returns `Other` instead of `Dock`. Share the full-screen editor footer geometry with pointer hit testing, including its minimum height.

## Verification

- Attempted `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev tui::pointer::tests --lib --locked`; canceled during `glass-browser` compilation before tests started. No independent test result is claimed. The task record reports 18 pointer tests and the responsive navigation test passing.
- Findings are based on the reviewed source paths and explicit contract mismatch; they are independently reproducible from the stated coordinates and layout conditions.

## Conclusion

**BLOCKED.** Both findings violate the task contract and need fixes plus regression coverage before this slice passes review.

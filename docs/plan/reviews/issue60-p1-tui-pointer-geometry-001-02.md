# Issue #60 P1 TUI pointer-geometry review 02

## Scope reviewed

Reviewed commit `c278afc6b322b03f9292aff887d6f4527c729c7b` against parent
`8f3aca105391ae0de606136e3963df2793d8d30e`. Re-read the task contract,
`docs/INDEX.md`, the Development TUI contract, and review 01. Re-audited the
F5 pointer geometry for shared shell/navigation/surface/list geometry, visible
and scrolled rows, empty collections, App inspector bounds, responsive layouts,
dynamic composer footer, More routes, and F1 overlay gating. Reviewed the full
follow-up diff and regression tests; did not modify implementation files.

## Findings

### P3 · non-blocking — Compact More-route geometry lacks a focused regression

The new More-route regression in `crates/glass-dev/src/tui/pointer.rs:1219-22`
exercises Mobile and Desktop, but not `TuiLayout::Compact`. Compact More
geometry has both a stacked branch and a horizontal branch in
`crates/glass-dev/src/tui/render.rs:3891-3909`, so adding at least one Compact
case would protect this responsive path explicitly. I found no current mismatch:
rendering calls `more_surface_geometry` and `more_route_geometry` at
`render.rs:3971-3973`, while hit testing derives its route geometry through the
same helpers at `render.rs:3944-3947`.

## Prior blockers rechecked

- **More-route render/hit geometry — resolved.** The route panel rectangle,
  padded content rectangle, and scroll offset now come from shared helpers
  (`render.rs:3927-3947`). Rendering clips and scrolls the route lines with
  that geometry (`render.rs:4033-4049`, `4104-4112`); hit testing accepts only
  the same content bounds and maps a visible row through the same offset
  (`render.rs:3949-3968`). The regression selects each visible route, exercises
  a nonzero phone scroll offset, and rejects border and horizontal-padding
  coordinates (`pointer.rs:1219-1264`).
- **Fullscreen-editor Composer footer — resolved.** The editor renderer and
  pointer path now share `fullscreen_editor_rows`; `pointer_footer_area` uses
  its fourth-row minimum while Composer overlays the full-screen editor
  (`render.rs:901-925`; `pointer.rs:188-208`). The new one-line Composer test
  verifies the first rendered dock row is `Dock` and the preceding row is not
  (`pointer.rs:1106-1128`).

The broader F5 paths remain consistent with the task contract: `hit_test`
rejects out-of-terminal coordinates and gates covered surfaces through the
active overlay (`pointer.rs:181-220`); visible list and entity hits continue to
use the shared renderer geometry. No P1 or P2 defect was found.

## Verification

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev tui::pointer::tests --lib --locked` — passed: 19 passed, 0 failed, 444 filtered out. This independently exercised the More-route and fullscreen-editor regressions along with the existing pointer tests.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev mouse_navigation_hits_use_the_rendered_panel_on_responsive_layouts --lib --locked` — passed: 1 passed, 0 failed, 462 filtered out.
- Source review confirmed the render and hit paths use the same route panel/content/scroll helpers and the same fullscreen-editor row split.

## Conclusion

**PASS.** The two review-01 blockers are fixed. The Compact-only More-route
regression gap is non-blocking because the shared geometry is used by both
render and hit paths; adding explicit Compact coverage is recommended.

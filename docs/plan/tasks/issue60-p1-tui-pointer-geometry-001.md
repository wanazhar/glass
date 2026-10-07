---
id: issue60-p1-tui-pointer-geometry-001
scope: glass-dev/tui-pointer-geometry
status: in-progress
depends-on: [issue60-p1-tui-overlay-priority-001]
---

# Issue #60 P1: derive TUI pointer hits from rendered geometry

## Objective

Resolve F5 by making pointer hit targets follow the layout actually rendered on
desktop, compact, and phone sizes. Prevent clicks from targeting rows or
entities that are not visible or do not exist.

## Context

- `crates/glass-dev/src/tui/pointer.rs`
- `crates/glass-dev/src/tui/render.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `crates/glass-dev/src/tui/state.rs`
- `docs/architecture/development-tui.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), finding F5
- [Source report #42](https://github.com/wanazhar/glass/issues/42)

## Contract

- Pointer hit testing uses the same shell, navigation, content-panel, and list
  geometry that rendering uses. Header, footer, composer, responsive layout,
  scroll, and padding offsets must not be independently re-created in the
  pointer path.
- Each returned file, Git, process, debugger, browser-entity, or More-route
  index must refer to a visible item in the rendered collection. Empty
  collections and non-list regions return `Other`; they never synthesize index
  zero.
- Surface navigation hit regions match the renderer's desktop and compact
  navigation areas and remain absent when the phone layout hides that sidebar.
- Existing command-menu and More-route shared geometry remains the source of
  truth. Overlay gating from F1 continues to prevent covered surfaces from
  receiving pointer input.

## Path

- `crates/glass-dev/src/tui/pointer.rs`
- `crates/glass-dev/src/tui/render.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `docs/architecture/development-tui.md`
- `docs/plan/README.md`
- `docs/plan/analysis/issue-60.md`
- `docs/plan/tasks/issue60-p1-tui-pointer-geometry-001.md`

## Verification

- Exercise hit testing at rendered list boundaries for desktop, compact, and
  phone layouts, including menu, More routes, navigation, file/Git/process/
  debug lists, and App inspector entities.
- Exercise composer footer geometry with short and multi-line composer input;
  rows inside the rendered dock map to `Dock` and rows outside map to the
  visible surface or `Other`.
- Verify empty browser entity lists and empty surface lists never return an
  invalid item index, and clicks outside a panel do not select its rows.
- Run focused pointer/render tests, package checks, formatting, documentation
  checks, and independent review.

## Validation evidence

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev tui::pointer::tests --lib --locked` — 18 passed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev mouse_navigation_hits_use_the_rendered_panel_on_responsive_layouts --lib --locked` — 1 passed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-dev --lib --bins --locked` — passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-issue60-f5-documentation.json` — passed; 0 current-claim failures.
- `python3 scripts/check-documentation-depth.py` — passed; 93 current guides routed/audited.
- `python3 scripts/check-tui-shortcuts.py` — passed; 15 implementation keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py --glass /home/ubuntu/work/glass/target/debug/glass --glass-browser /home/ubuntu/work/glass/target/debug/glass-browser` reports MCP inventory drift: live development tool names differ from
  `crates/glass-dev/tests/fixtures/client-conformance-v1.json`; the schema budget lacks
  `| Negotiated tools | 177 |` and ``| Serialized `tools` array | 76,967 UTF-8 bytes |``.
  This TUI-only F5 change does not touch the MCP inventory or schema budget.

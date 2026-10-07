---
id: issue60-p1-tui-pointer-geometry-001
scope: glass-dev/tui-pointer-geometry
status: complete
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
- `docs/plan/reviews/issue60-p1-tui-pointer-geometry-001-01.md`
- `docs/plan/reviews/issue60-p1-tui-pointer-geometry-001-02.md`

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

## Review follow-up: shared More and fullscreen-editor geometry

- Independent review `docs/plan/reviews/issue60-p1-tui-pointer-geometry-001-01.md`
  was committed as `8f3aca10` and blocked on the two geometry gaps below; this
  follow-up addresses both findings and leaves the task `in-progress` pending
  another review.
- More route panel placement, padded content bounds, and scroll offset are now
  derived once and consumed by both rendering and pointer hit testing. The
  route regression checks every selected row on phone and desktop, exercises a
  short phone viewport with a nonzero route scroll offset, and rejects panel
  borders and horizontal padding.
- Fullscreen-editor rendering and pointer hit testing now share the same row
  split, including the Composer dock's four-row minimum. A one-line Composer
  over the fullscreen Code editor regression checks that the first rendered
  dock row maps to `Dock`.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev tui::pointer::tests --lib --locked` — 19 passed after the review fixes.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-dev --lib --bins --locked` — passed; only existing `glass-browser` dead-code warnings.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-issue60-f5-review-documentation.json` — passed; 0 current-claim failures.
- `python3 scripts/check-documentation-depth.py` — passed; 93 current guides routed/audited and 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py` — passed; 15 implementation keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py --glass /home/ubuntu/work/glass/target/debug/glass --glass-browser /home/ubuntu/work/glass/target/debug/glass-browser` — remains blocked by the same MCP fixture/schema-budget drift listed above; the live measurements remain 177 tools and 76,967 serialized bytes.
- Post-merge `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --all-targets --locked` did not complete. In the unchanged `glass-browser` crate, `content_process_font_destination_uses_the_native_font_loader` failed at `content_process.rs:25757` (0 resources, expected 1), and `script_text_content_replaces_subtree_and_detaches_old_nodes` failed at `dom.rs:16896` (4 elements, expected 2). The run then aborted on stack overflow in `cli::args::tests::agent_readiness_commands_are_explicit`; that isolated test passes with `RUST_MIN_STACK=8388608`. These paths are outside the F5 TUI diff. Focused F5 tests and independent review passed.

## Independent review

- Review 01 found two blocking geometry mismatches. Both were fixed in
  `c278afc6` and independently rechecked in
  [review 02](../reviews/issue60-p1-tui-pointer-geometry-001-02.md), which
  passed. Review 02 records a non-blocking P3 note that the More-route test
  lacks an explicit Compact-only case; rendering and hit testing use the same
  geometry helpers for that layout.
- Finding F5 is complete.

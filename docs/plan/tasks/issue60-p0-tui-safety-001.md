---
id: issue60-p0-tui-safety-001
scope: glass-dev/tui-trust-and-target-privacy
status: pending
depends-on: [issue60-p0-governance-001]
---

# Issue #60 P0: TUI trust routing and redacted target filtering

## Objective

Resolve findings F8 and F35. While a workspace is untrusted, all desktop and
phone navigation/action routes must keep Trust authoritative. Target-picker
filtering must use only information Glass renders as safe; query and fragment
values must not affect target match counts.

## Context

- `docs/workspace-trust.md`
- `docs/architecture/development-tui.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), findings F8/F35
- Source reports [#42](https://github.com/wanazhar/glass/issues/42) and [#46](https://github.com/wanazhar/glass/issues/46)

## Contract

- Trust gating is derived from the workspace's authoritative trust state and
  applies equally to number keys, printable surface keys, phone shortcuts,
  menus, and action dispatch.
- A secret present only in a URL query or fragment cannot change the target
  picker's visible match set or count.

## Path

- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/command.rs`
- `crates/glass-dev/src/tui/render.rs`
- `docs/workspace-trust.md` if the routing contract changes

## Verification

- Add interaction coverage for each desktop/phone route that can leave Trust
  while untrusted.
- Add a target-picker regression proving query/fragment-only secrets do not
  alter visible matches or counts.
- Exercise the TUI state transition through the real routing functions.

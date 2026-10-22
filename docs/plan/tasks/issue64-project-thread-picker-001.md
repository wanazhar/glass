---
id: issue64-project-thread-picker-001
scope: issue-64-project-local-thread-navigation
status: in-progress
depends-on: [issue63-thread-isolation-001, issue63-durable-threads-001]
---

# Project-local thread picker

## Objective

Make durable project conversations discoverable from the Agent surface and
switch among them without dispatching a prompt or losing per-thread composer
state.

## Context

- [Glass delivery plan](../README.md)
- [Thread isolation](issue63-thread-isolation-001.md)
- [Durable project threads](issue63-durable-threads-001.md)
- [Development TUI architecture](../../architecture/development-tui.md)

## Paths

- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `crates/glass-dev/src/tui/overlay.rs`
- `crates/glass-dev/src/tui/render.rs`
- `crates/glass-dev/src/tui/bindings.rs`
- `crates/glass-dev/src/tui/shortcuts.json`
- `crates/glass-dev/tests/tui_pty.rs`
- `docs/architecture/development-tui.md`

## Acceptance

- [x] Keyboard opens a project-local thread picker from Agent at 80×24 and
      50×16; rows display a readable title and current honest lifecycle status.
- [x] Up/Down and Enter select the exact conversation without dispatching a
      prompt; Escape leaves the current conversation selected.
- [x] Switching among three registered project threads preserves each draft and
      cursor; a reducer result for thread A does not steal focus from C.
- [ ] A fake-runtime journey keeps three concurrent active tasks independent
      through background completion and the remaining project task lifecycle.
- [x] Recovery-pending/unavailable rows remain distinguishable and no status
      implies that a prompt was resumed or replayed.
- [x] Focused reducer/render tests and an allocated-PTY journey pass, along with
      formatting, strict scoped Clippy, and documentation checks.

## Evidence

The current picker opens from Agent with `Ctrl-Shift-T`, lists resident
conversation roles and lifecycle labels from the latest workspace snapshot,
and selects without sending. `project_thread_picker_lists_status_and_selects_without_losing_draft`
passes, including recovery-pending display, three-thread draft/cursor
restoration, and a late result for the first thread while the third remains
selected.
`project_thread_picker_is_discoverable_at_desktop_and_compact_sizes` passes
with the fake Core fixture at 80×24 and 50×16. Formatting, strict scoped
Clippy, shortcut validation, docs depth, docs coverage, release-truth, and
diff checks pass. The issue-level three-concurrent-task/fake-runtime journey
remains open; this picker task uses persisted synthetic thread records and a
reducer result event, while the allocated PTY covers keyboard navigation and
selection at both sizes.

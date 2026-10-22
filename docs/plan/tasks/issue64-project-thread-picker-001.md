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
- [x] A fake-runtime journey keeps three concurrent active tasks independent
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
`spawned_resident_agent_dispatches_an_offline_prompt_from_the_composer` also
passes with the fake Core runtime: it starts a resident agent, waits until the
project picker reports it idle, selects that conversation, opens its composer,
and sends a prompt. The fixture records the matching `turn/prompt`; the
persisted attempt records the engine request ID and reaches `settled`. This
proves a single selected conversation can complete a real adapter round trip
through the TUI. It does not prove isolation among simultaneously active Core
tasks; the dedicated three-task journey below covers that behavior.
`project_thread_picker_is_discoverable_at_desktop_and_compact_sizes` passes
with the fake Core fixture at 80×24 and 50×16. Formatting, strict scoped
Clippy, shortcut validation, docs depth, docs coverage, release-truth, and
diff checks pass. `three_fake_core_tasks_complete_without_stealing_selected_thread`
starts three resident Core agents and sends a prompt through each selected
conversation. Fixture gates hold all three requests active at once; the
persisted ledger shows separate threads, distinct client message IDs, and
working attempts with request IDs. Releasing alpha while gamma is selected
does not replace gamma's conversation with alpha's response; releasing beta
and gamma then settles all three attempts in their owning threads. The fixture
gate is enabled only by this test's marker, so ordinary offline adapter tests
retain their immediate response behavior.

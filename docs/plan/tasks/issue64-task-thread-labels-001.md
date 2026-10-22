---
id: issue64-task-thread-labels-001
scope: issue-64-project-thread-identity
status: in-progress
depends-on: [issue64-project-thread-picker-001]
---

# Task identity in the Project Threads picker

## Objective

Make task-assigned resident conversations distinguishable when multiple
project tasks use the same agent role, while preserving the existing role label
for manually spawned agents and safely handling missing or unassigned tasks.

## Context

- [Project-local thread picker](issue64-project-thread-picker-001.md)
- [Development TUI architecture](../../architecture/development-tui.md)

## Paths

- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/snapshot.rs`
- `docs/architecture/development-tui.md`

## Acceptance

- [x] Both picker projections label task-assigned agents with role, task title,
      and task ID.
- [x] Missing assignments and manually spawned agents retain their role-only
      label without failing refresh.
- [x] A focused projection test proves two same-role assigned agents are
      distinguishable and the fallback remains role-only.
- [x] Formatting, focused tests, strict scoped Clippy, documentation checks,
      and diff checks pass.

## Evidence

`project_thread_picker_items` joins task snapshots to resident agents by the
scheduler's `assigned_agent` field in both the direct refresh and immutable
snapshot paths. Assigned conversations show `role · task-id · task title`;
unassigned tasks do not label unrelated agents, and agents with no assignment
keep the existing role-only title. The focused
`task_assigned_threads_show_task_identity_and_other_threads_keep_role` unit test
checks two `developer` assignments, one unassigned task, and a manually spawned
agent.

The focused projection test and existing
`project_thread_picker_lists_status_and_selects_without_losing_draft` test pass
with `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target`. Strict scoped Clippy
(`cargo clippy -p glass-dev --lib --locked -- -D warnings`), workspace fmt,
`git diff --check`, documentation depth, release-truth, and coverage checks
pass. Coverage used the existing shared root binaries through explicit
`--glass` and `--glass-browser` paths because this isolated worktree has no
local `target/debug` binaries. No disk cleanup or global build was performed.

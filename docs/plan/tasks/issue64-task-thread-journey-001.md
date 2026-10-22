---
id: issue64-task-thread-journey-001
scope: issue-64-task-scheduler-project-thread-journey
status: completed
depends-on: [issue64-task-thread-labels-001, issue64-three-thread-journey-001]
---

# TaskScheduler project-thread journey

## Objective

Exercise task creation through the TUI, task-to-agent assignment, task-labeled
project conversations, per-thread draft and cursor ownership, and background
completion without focus theft. This is a separate TaskScheduler acceptance
journey from the manual-agent three-thread journey.

## Context

- [Development TUI architecture](../../architecture/development-tui.md)
- [Task identity in the Project Threads picker](issue64-task-thread-labels-001.md)
- [Three-thread homespace journey](issue64-three-thread-journey-001.md)
- [Glass delivery plan](../README.md)

## Paths

- `crates/glass-dev/tests/tui_pty.rs`
- `crates/glass-dev/src/tui/state.rs`
- `docs/plan/README.md`

## Acceptance

- [x] The TUI creates alpha, beta, and gamma tasks through the governed task
      tool path, with fake Core held at three independent prompt gates.
- [x] Task snapshots expose each task's title, assigned agent, and running or
      succeeded state; the Project Threads picker joins the same assignment to
      the matching task ID/title label and opens the corresponding durable
      conversation record.
- [x] Each task conversation retains its own draft and cursor after switching
      among all three same-role tasks.
- [x] Alpha settles while gamma remains selected. The task state updates to
      succeeded, the picker selection and gamma conversation stay focused, and
      beta/gamma remain running with their durable drafts intact.
- [x] The allocated PTY journey passes against a freshly built debug TUI
      binary, with strict scoped Clippy, formatting, Python fixture syntax,
      documentation checks, and diff validation.

## Verification

Build the current development TUI into the shared target with
`cargo build -p glass-dev --bin glass --locked`, then run the focused test with
`GLASS_E2E=1` and explicit `GLASS_E2E_BINARY=/home/ubuntu/work/glass/target/debug/glass`.
Run strict `cargo clippy -p glass-dev --test tui_pty --locked -- -D warnings`,
workspace formatting, fixture `py_compile`, documentation coverage/depth/
release-truth/TUI-shortcut checks, and `git diff --check`.

## Evidence

The PTY journey creates three tasks through the TUI's governed generic tool
command with `verification: {"kind":"settled"}`, confirming each creation
through the normal one-use prompt. Fake Core holds `alpha-job`, `beta-job`, and
`gamma-job` at independent gates. The Tasks projection shows all three as
running with their titles and assigned agent IDs. The Project Threads picker
joins those same assignments to `developer · task-id · title`; selecting each
row opens its corresponding conversation and persists a distinct draft and
non-terminal cursor in a separate durable thread record. Reopening a thread
previously moved the composer cursor to the end; `open_composer` now preserves
the selected thread's cursor, and the revisit PTY inserts then deletes a marker
at that restored position while checking the durable record. A focused unit test
also covers reopening a persisted cursor. Releasing alpha
updates its task to `succeeded` while beta and gamma remain `running`; after
returning to Agent, gamma is still selected, its conversation stays visible,
and all three drafts/cursors and run records remain intact.

The focused journey and cursor restore unit test passed:

```text
CARGO_TARGET_DIR=/home/ubuntu/work/glass/target GLASS_E2E=1 \
  GLASS_E2E_BINARY=/home/ubuntu/work/glass/target/debug/glass \
  cargo test -p glass-dev --test tui_pty \
  task_scheduler_threads_keep_task_identity_drafts_and_background_focus \
  --locked -- --nocapture
1 passed; 0 failed
```

```text
CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib \
  composer_draft_reopens_by_thread_id_after_agent_id_changes --locked
1 passed; 0 failed
```

The binary was freshly built from this worktree with
`CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo build -p glass-dev --bin glass --locked`;
`/home/ubuntu/work/glass/target/debug/glass --version` reports `glass 0.3.14`.
This was a debug build, not a release build. The initial PTY size was increased
to 120×50 so all three task rows are visible together. The behavior fix is
limited to preserving the active cursor in `open_composer`; no fixture changes
were needed. Strict scoped Clippy, workspace formatting, fixture syntax,
documentation coverage/depth/release-truth/TUI-shortcut checks, and diff
validation passed. Documentation coverage used the shared target binaries via
explicit `--glass` and `--glass-browser` paths.

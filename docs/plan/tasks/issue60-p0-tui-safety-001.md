---
id: issue60-p0-tui-safety-001
scope: glass-dev/tui-trust-and-target-privacy
status: complete
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

- The initial Trust surface cannot be bypassed through desktop or phone
  shortcuts, pointer navigation, menus, or command/palette dispatch. After the
  explicit Open untrusted choice, safe review navigation and file preview are
  available; project writes, including editor checkpoints, workflow drafts,
  buffer edits/saves, and proposal application, require live workspace trust.
- Every project execution gate reads authoritative workspace trust directly,
  rather than relying on the display snapshot.
- A secret present only in a URL query or fragment cannot change the target
  picker's visible match set or count.

## Path

- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/command.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `crates/glass-dev/src/tui/pointer.rs`
- `crates/glass-dev/src/tui/render.rs`
- `crates/glass-dev/src/development/project.rs`
- `docs/workspace-trust.md` if the routing contract changes

## Verification

- Add interaction coverage for each desktop/phone route that can leave Trust
  while untrusted.
- Add a target-picker regression proving query/fragment-only secrets do not
  alter visible matches or counts.
- Exercise actual global shortcut dispatch, file-picker submission, command
  dispatch, and pointer navigation through their TUI routing functions.
- Prove Open untrusted supports file preview while the editor, file saves, and
  proposal application remain blocked and existing unsaved content is kept.
- Prove a path containing `.` resolves to an existing dirty buffer during
  preview; explicit discard still reloads from disk and does not resurrect the
  discarded buffer on reopen.

## Validation evidence

- `cargo fmt --all -- --check` and `git diff --check` pass.
- `cargo test -p glass-dev --lib --locked tui::` passes all 213 TUI tests,
  including a read-only Open untrusted preview and live-trust editor-write
  regression, workflow-recording guards after closing and reopening the
  project untrusted, and app-attach checks that prevent checkpoint writes.
- `cargo test -p glass-dev --lib --locked untrusted_workflow_recording_cannot_start_capture_or_write_draft` passes.
- `cargo test -p glass-dev --lib --locked untrusted_app_attach_does_not_persist_editor_checkpoint` passes.
- `cargo test -p glass-dev --lib --locked opening_a_path_alias_reuses_the_existing_dirty_buffer` passes.
- `cargo test -p glass-dev --lib --locked editor_exit_prompts_save_discard_and_discard_quit` passes; discard reloads disk content and quit retains saved content.
- `cargo test -p glass-dev --lib --locked initial_trust_shortcuts_block_app_jump_and_file_picker_submission` passes, covering Ctrl-P/Ctrl-G dispatch and stale picker submission.
- `cargo build -p glass-browser --bin glass-native-content-worker --locked` succeeds, and `cargo test -p glass-dev --lib --locked` passes all 410 tests.
- `cargo check -p glass-dev --lib --bins --locked` passes.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-issue60.json`, `python3 scripts/check-documentation-depth.py`, and `python3 scripts/check-tui-shortcuts.py` pass.
- `scripts/check-documentation-coverage.py` still reports the live development
  MCP inventory (177 tools, 76,967 serialized bytes) differs from the
  conformance fixture (346 entries) and schema-budget record. This is outside
  the TUI files changed here and needs follow-up in the Issue #60 MCP contract
  task.

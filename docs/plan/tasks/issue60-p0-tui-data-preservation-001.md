---
id: issue60-p0-tui-data-preservation-001
scope: glass-dev/tui-editor-agent-data-preservation
status: complete
depends-on: [issue60-p0-tui-safety-001]
---

# Issue #60 P0: preserve TUI editor and chat state

## Objective

Resolve findings F6, F16, F18, F63, and F99. Keep unsaved editor buffers
behind one authoritative exit guard; bind FIM ghost text to its originating
buffer and revision; make accepted suggestions visible and bounded on every
editor path; and reconcile chat messages by stable request identity rather
than message text.

## Context

- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), findings
  F6/F16/F18/F63/F99
- Source reports [#42](https://github.com/wanazhar/glass/issues/42),
  [#45](https://github.com/wanazhar/glass/issues/45), and
  [#50](https://github.com/wanazhar/glass/issues/50)

## Contract

- Every exit route, including Escape and nested prompts, passes through one
  state machine that cannot discard unsaved edits without the normal guard.
- FIM results are accepted only by the buffer and revision that requested
  them; stale results are dropped visibly and never inserted into another
  buffer.
- Every editor mode renders the suggestion consistently, and a provider result
  exceeding limits is rejected or explicitly reported instead of silently
  truncated or accepted invisibly.
- Pending chat messages are reconciled by stable job/message identity, so two
  distinct identical messages remain distinct.

## Path

- `crates/glass-dev/src/tui/state.rs`
- Editor and agent/chat state modules under `crates/glass-dev/src/tui/`
- Pi and provider FIM request/response integration
- Relevant TUI and editor contract documentation

## Verification

- Cover Escape, quit, nested prompt, and unsaved-buffer transitions through
  the real state-routing functions.
- Cover stale and current FIM results across path/revision changes, every
  editor mode, and provider size boundaries.
- Cover two identical chat messages with distinct identities through pending
  reconciliation and completion.
- Record interaction-level TUI evidence for the visible suggestion and exit
  guard behavior.

## Verification evidence

- `cargo test -p glass-dev --lib --locked` — 419 passed.
- Focused regressions passed for
  `request_with_id_returns_the_workers_pi_request_identifier`,
  `pi_fim_response_requires_the_exact_request_identifier`,
  `quitting_after_gp_still_guards_a_dirty_editor_buffer`, and
  `quitting_selects_the_dirty_buffer_when_another_buffer_is_focused`.
- `cargo check -p glass-dev --lib --bins --locked` — passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- `cargo build -p glass-browser --bin glass-native-content-worker --locked` —
  passed for the full library test environment.

---
id: issue60-p0-tui-data-preservation-001
scope: glass-dev/tui-editor-agent-data-preservation
status: pending
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

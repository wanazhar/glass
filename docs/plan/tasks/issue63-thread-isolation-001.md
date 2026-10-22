---
id: issue63-thread-isolation-001
scope: issue-63-live-agent-thread-isolation
status: done
depends-on: [core-provider-controls-001]
---

# Live resident-agent thread isolation

## Objective

Make each live resident agent a distinct TUI conversation context. Switching
agents must switch transcript and preserve that thread's composer draft/cursor;
late asynchronous results must remain attached to their originating agent and
must not steal focus or overwrite another thread's draft.

## Context

- `docs/plan/analysis/issue63-thread-isolation.md`
- `docs/architecture/development-tui.md`
- `docs/development-runtime.md`
- `docs/plan/tasks/core-provider-controls-001.md`
- `AGENTS.md`

## Scope

- `crates/glass-dev/src/tui/snapshot.rs`
- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/projection.rs`
- `crates/glass-dev/src/tui/mod.rs` and `command.rs` only for ownership-safe
  selected-agent behavior
- Focused TUI snapshot/state/projection tests
- Current TUI and runtime docs

Do not persist AgentId-keyed drafts across restart, add a durable thread/run
registry, change Core session persistence, add an editable follow-up queue or
attention inbox, or redesign the TUI layout. AgentId is process-local; that
limitation must remain visible in docs and task evidence.

## Acceptance

- [x] Snapshot conversation data is grouped by AgentId; each selected live
      thread displays only its own messages and tool activity.
- [x] Switching between live agents saves and restores independent composer
      text and character cursor; no selected agent retains a separate
      unthreaded draft.
- [x] Every pending chat send records its owning AgentId. A first, unthreaded
      send is attached to the AgentId returned by its successful bootstrap.
- [x] A late successful result for thread A updates only A's pending message
      and transcript, without selecting A or changing thread B's focus/status
      or composer draft/cursor.
- [x] A late failed result remains recoverable in thread A and cannot replace
      thread B's draft.
- [x] A behavioral regression uses two live-agent identities with distinct
      transcripts and drafts, selects both, then delivers a late A result while
      B is selected; it verifies owner, content, cursor, and focus.
- [x] Current TUI/runtime docs describe live per-agent isolation and clearly
      state that AgentId-based draft state is not durable across restart.
- [x] Focused `glass-dev` tests, formatting, strict scoped Clippy, docs
      coverage/depth/release-truth/version checks, and diff review pass using
      `/home/ubuntu/work/glass/.worktrees/issue63-mixed-tools-001/target`.
- [x] Commit locally with a Conventional Commit. Do not push, release, restart
      services, modify installed binaries, or close issue #63.

## Verification

Verification completed in the isolated worktree with the shared target
`/home/ubuntu/work/glass/.worktrees/issue63-mixed-tools-001/target`:

- `cargo test -p glass-dev --lib tui::state::tests --locked`: 111 passed.
- `cargo test -p glass-dev --lib tui::snapshot::tests --locked`: 2 passed.
- Focused regressions passed for late success/failure, bootstrap ownership,
  snapshot partitioning, and the two affected existing Agent chat tests.
- `cargo fmt --all -- --check`, `cargo clippy -p glass-dev --lib --locked --
  -D warnings`, and `git diff --check` passed.
- Documentation coverage, depth, release-truth, and version-sync validators
  passed using the existing root inventory binaries.
- A broader `cargo test -p glass-dev --lib --locked` run reported 580 passing
  and four failures before two affected UI fixtures were corrected. Those two
  focused UI tests pass after correction. The other two failures were unrelated:
  the native content-worker binary was absent and an experiment fake-runtime
  fixture supplied an incompatible protocol version. The full suite was not
  rerun after the fixture corrections.

`AgentId` and its drafts remain process-local. Restart recovery requires the
separate Glass-owned persistent thread/run identity work and is not claimed by
this checkpoint.

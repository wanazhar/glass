---
id: issue63-durable-threads-001
scope: issue-63-durable-project-threads
status: in-progress
depends-on: [issue63-thread-isolation-001]
---

# Durable project thread ledger

## Objective

Persist Glass-owned `ThreadId` and `RunId` identities plus the actual Core
session reference. On workspace reopen, restore records as recovery-pending
entries in the existing Agent picker, never as live workers. Do not replay turns
or create replacement sessions. Exact Core session describe/resume is a
separate dependent checkpoint.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/issue63-durable-project-threads.md`
- `docs/plan/analysis/issue63-thread-isolation.md`
- `docs/plan/tasks/issue63-thread-isolation-001.md`
- `docs/architecture/development-tui.md`
- `docs/development-runtime.md`
- `crates/glass-dev/src/agents.rs`
- `crates/glass-dev/src/resident_harness.rs`
- `crates/glass-dev/src/core_runtime.rs`

## Paths

- `crates/glass-dev/src/thread_store.rs` (new), module registration
- `crates/glass-dev/src/agents.rs`
- `crates/glass-dev/src/resident_harness.rs`
- `crates/glass-dev/src/core_runtime.rs`
- `crates/glass-dev/src/tui/snapshot.rs`
- `crates/glass-dev/src/tui/state.rs` only for truthful recovery-pending
  selection/send behavior
- Focused tests and current runtime/TUI docs
- `docs/plan/README.md` and this task record

Do not persist `AgentId`, job IDs, provider credentials/config, transcripts,
pending host tools, or follow-up queues. Do not add an implicit Core resume or
fallback to `session/new`. Do not implement queue editing or attention inbox.
Do not touch protocol/TUI ownership behavior or perform broad formatting.

## Acceptance

- [ ] A Glass-owned versioned project thread ledger is stored under `.glass`,
      with explicit project identity and bounded schema validation.
- [ ] Writes are private and atomic; invalid schema/identity and I/O failures
      are surfaced without overwriting the prior registry.
- [ ] Stable random ThreadId and RunId are created; AgentId and worker/job IDs
      remain process-local and absent from serialized data.
- [ ] The session reference is captured from an initialized Core runtime and is
      persisted before any prompt can be accepted.
- [ ] Restart-like reopen retains thread/run/session references but classifies
      prior nonterminal runs as recovery-pending; no work is replayed.
- [ ] This checkpoint does not yet persist drafts or reconnect to a Core
      session; task docs and UI must say so clearly.
- [ ] Recovered rows appear in the existing Agent picker; send attempts fail
      explicitly while recovery is pending, without creating a new session.
- [ ] Focused tests cover round-trip/reopen, atomic/failure/schema behavior,
      adapter session ID capture, pending-state projection, and no prompt replay.
- [ ] Current TUI/runtime docs distinguish durable Glass identity from
      ephemeral AgentId and state that exact Core session reattachment is not
      delivered by this task.
- [ ] Focused `glass-dev` tests, format, strict scoped Clippy, docs validators,
      and diff review pass using the existing shared target.
- [ ] Commit locally with a Conventional Commit; do not push, release, tag,
      restart services, globally install, or close issue #63.

## Verification

Pending implementation and independent review.

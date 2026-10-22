---
id: issue63-durable-threads-001
scope: issue-63-durable-project-threads
status: in-progress
depends-on: [issue63-thread-isolation-001]
---

# Durable project thread ledger

## Objective

Persist Glass-owned `ThreadId` and `RunId` identities plus the actual Core
session reference. On workspace reopen, restore initialized nonterminal runs as
recovery-pending entries in the existing Agent picker, show pre-initialization
and queued-but-never-started work as unavailable, and preserve terminal
classifications. Rows are never live workers. Do not replay turns or create
replacement sessions. Exact Core session
describe/resume is a separate dependent checkpoint.

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
- `crates/glass-dev/src/tui/snapshot.rs` for recovery row projection
- Focused tests and current runtime/TUI docs
- `docs/plan/README.md` and this task record

Do not persist `AgentId`, job IDs, provider credentials/config, transcripts,
pending host tools, or follow-up queues. Do not add an implicit Core resume or
fallback to `session/new`. Do not implement queue editing or attention inbox.
Do not touch protocol/TUI ownership behavior or perform broad formatting.

## Acceptance

- [x] A Glass-owned versioned project thread ledger is stored under `.glass`,
      with explicit project identity and bounded schema validation.
- [x] Writes are private and atomic; invalid schema/identity and write failures
      are surfaced without overwriting the prior registry.
- [x] Stable random ThreadId and RunId are created; AgentId and worker/job IDs
      remain process-local and absent from serialized data.
- [x] The session reference is captured from an initialized Core runtime and is
      persisted before any prompt can be accepted.
- [x] Restart-like reopen retains thread/run/session references, classifies
      initialized nonterminal runs as recovery-pending, shows uninitialized runs
      and never-started queued work as unavailable, and preserves terminal rows;
      no work is replayed.
- [x] This checkpoint does not yet persist drafts or reconnect to a Core
      session; task docs and UI must say so clearly.
- [x] Recovered rows appear in the existing Agent picker; send attempts fail
      explicitly while recovery is pending, without creating a new session.
- [x] Focused tests cover round-trip/reopen, concurrent writes, atomic/failure,
      schema/identity behavior, private paths, adapter session ID capture,
      pending-state projection, terminal rows, and no prompt replay.
- [x] Current TUI/runtime docs distinguish durable Glass identity from
      ephemeral AgentId and state that exact Core session reattachment is not
      delivered by this task.
- [x] Focused `glass-dev` tests, format, strict scoped Clippy, docs validators,
      and diff review pass using the existing shared target.
- [x] Commit locally with a Conventional Commit; do not push, release, tag,
      restart services, globally install, or close issue #63.

## Verification

Passed using shared target `/home/ubuntu/work/glass/.worktrees/issue63-mixed-tools-001/target`:

- `cargo fmt --all -- --check`
- `cargo test -p glass-dev --lib thread_store --locked` (8 passed)
- Focused `reopened_thread_rows_are_truthful_and_never_accept_prompts`,
  `worker_persists_core_session_before_ready_notification`,
  `terminal_cancel_classification_survives_registry_reopen`,
  `snapshot_worker_publishes_and_applies_without_blocking_ui`, and
  `protocol_capability_not_package_version_decides_compatibility` tests
- `cargo clippy -p glass-dev --lib --bins --locked -- -D warnings`
- `scripts/check-documentation-coverage.py` with existing root Glass binaries,
  `scripts/check-documentation-depth.py`, and
  `scripts/check-release-documentation.py`

Independent review remains with the parent agent. No broad suite was run.

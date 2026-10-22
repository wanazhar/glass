---
id: ext-lifecycle-001
scope: roadmap item 32 — extension cancellation, shutdown and nested-call lineage contracts
status: implemented
depends-on: []
---

# Extension cancellation, shutdown and call lineage

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Roadmap item 32. Closes the verified cancellation and lineage gaps. The
shutdown hook already exists and must keep working; this task adds the missing
contracts around it rather than replacing it.

## Objective

1. **Make cancellation real.** `Extension::spawn_cancellable` accepts a cancel
   flag and `finish_startup` then replaces it with a fresh flag that nothing can
   ever set. The `Arc<AtomicBool>` held by `modal::task` is dropped on the floor
   at `addons.rs`. Wire a cancel token that survives startup and reaches a
   running tool call: `Registry::run_tool`, `run_tool_with_input` and
   `run_command` currently accept no token. A cancelled call must stop waiting
   promptly, report cancellation distinctly from an extension failure, and leave
   an accurate transcript of completed operations.
2. **Nested-call lineage.** `RunTool` and `RunToolInput` in the protocol carry
   no `call_id`, so nothing links an extension's nested work back to the model
   tool call that caused it. Carry the originating call id through the nested
   call and result messages, and bound the nesting depth. A cycle or an
   over-deep chain must fail with an explicit error rather than recursing.
3. **Shutdown contract.** `Registry::shutdown` currently discards per-extension
   errors. Report each extension's outcome, keep shutdown idempotent, and keep
   the existing process-group kill and `Drop` guarantee. Disabling one extension
   must not disturb a running session's other extensions.

## Context

- [Plugins](plugins-001.md) — extension loading and capability model.
- [MCP bridge](mcp-bridge-001.md) — nested calls from extensions into host tools.
- [Tooling foundation](tooling-foundation-001.md) — shared tool contract and
  cancellation semantics for items 38–43.
- `docs/INDEX.md` — document index.

## Path

Own these paths only.

- `crates/glass/src/ext/host.rs`
- `crates/glass/src/ext/mod.rs`
- `crates/glass/src/ext/protocol.rs`
- `crates/glass/src/addons.rs`
- `crates/glass/src/ext/**/tests` or new unit tests in the same files
- `docs/plan/tasks/ext-lifecycle-001.md`

Do not edit `crates/glass/src/main.rs`, `crates/glass/src/agent.rs`,
`crates/glass/src/tools.rs` or `crates/glass/src/session.rs`; another task owns
those in this wave.

## Constraints

- No new dependency and no new startup work. An idle session must not pay for
  this task.
- Cancellation is cooperative. Never report "cancelled" for work that actually
  completed, and never silently discard a result the extension already returned.
- Nested lineage must be bounded; an unbounded parent chain is a denial-of-
  service vector against the extension host.
- Shutdown stays idempotent and keeps killing the process group, including
  TERM-ignoring descendants. This is existing protection and must not regress.
- An extension that cannot be cancelled promptly falls back to the existing
  timeout and process kill, and must say which path was taken.

## Verification

- Unit tests in the owned files for: cancel reaching a running call, cancel
  before dispatch, cancel reported distinctly from failure, lineage id
  round-trip, depth bound rejection, cyclic lineage rejection, per-extension
  shutdown error reporting, and shutdown idempotence.
- `cargo clippy --workspace --all-targets -- -D warnings` green.
- `cargo test --locked --workspace` green.
- Confirm no `cancel` flag is dropped on the floor: a cancelled `/extensions`
  startup still cancels.

## Implementation notes

The extension host now owns a `Cancel` token that outlives the handshake:
`finish_startup` clears the flag the startup modal set and returns the live
 token instead of replacing it with a permanently clear one. Every wait polls
that token — and the per-call token — inside the poll loop, so a cancel stops
the wait within one poll interval rather than after the reply timeout.

Calls report a `CallOutcome`, which separates a completed result, an
unavailable tool, a cancellation (with the path it took) and a genuine
failure. A cancel drains the pipe for one bounded window first, so a reply the
extension already finished writing is reported as finished rather than
cancelled.

`Request::RunTool` / `RunToolInput` carry the originating `call_id` and the
full `CallLineage`. Nested dispatch extends the host's chain, refuses a
claimed lineage that re-parents or loops, and stops at
`MAX_NESTED_CALL_DEPTH` with an explicit error instead of recursing.

`Registry::shutdown` reports one outcome per extension, keeps the
process-group kill and the `Drop` guarantee, and is idempotent at both the
registry and the single-extension level. `disable` stops only the named
extension.

The scoped entry points (`run_tool_scoped`, `run_tool_with_input_scoped`,
`run_command_scoped`, `cancel_token`, `cancel`, `cancel_all`, `force_stop`,
`last_shutdown`, `CallOutcome::describe`) are the wire-up surface the agent
loop consumes; the unscoped wrappers keep the current call sites compiling.
Wiring `agent.rs` to build a `CallScope` per model tool call is the follow-up
that owns that file.

---
id: durability-001
scope: session logs, agent turn records, live sidecar, serve protocol
status: done
depends-on: []
---

## Objective

The five durability mechanisms borrowed from pi (`research/pi-durability-2026-10.md`):
the effect sandwich, log repair, throttled partials, cursor reattach, and
idempotent submissions — plus the three latent bugs they exposed.

## Context

**Log integrity (`session.rs`).** `repair_log` runs before every replay:
an invalid torn final line (no trailing newline) is truncated at its exact original byte boundary, preserving blank lines and CRLF; a valid final record without a newline gets its missing separator — without that, the
next append glued itself to the fragment and both records were lost
silently; a complete unreadable line is corruption, so the original is
backed up beside the log, the line is dropped, and a `Meta` records the
repair. Unknown `k` values remain forward compatibility, not corruption.
`log_fsync = "off" | "always"` sets the per-record durability policy
(process-crash consistency by default; power-loss survival opt-in).

**Effect sandwich (`session.rs`, `agent.rs`).** The `ToolCall` record is
already appended before a tool executes, so it *is* the durable intent. On
open, `interrupted_calls` answers every call with no recorded result with a
synthetic `ToolResult` — never a re-run — worded by replay policy (`state`
safe; everything else unsafe: "may have partially run"). This keeps
`tool_use` paired with `tool_result` for the provider and stops `/retry`'s
completed-result reuse from silently re-executing a command that may have
half-landed. No auto-resume, deliberately: glass-core surfaces the
interruption and lets the user spend tokens via `/retry`.

**Committed partials (`partial.rs`, `agent.rs`).** An `Agent` owns a
throttled `<session>.live.json` writer (`live_flush_ms`, default 250, 0
disables): streamed assistant text (tail-capped 64 KB), the tool in flight,
and the retry attempt + deadline (the committed retry clock). Tool start is
a forced flush because it is a phase boundary — the throttle alone would
leave the last delta unwritten for exactly as long as the tool runs. The
writer is dropped with the agent, so a file that survives means a kill; on
open, `partial::recover` records the partial as the assistant message it
was becoming (8 KB tail) plus a visible `Meta`, and a final-flush race is
detected by containment in an assistant text from the same raw user turn.
Recoverable sidecars are removed after checked recovery writes succeed; a
write failure aborts open and retains the sidecar.

**Cursor reattach and dedupe (`serve.rs`).** `attach` accepts `since=<raw
record count>`; the replay frame carries `cursor` and `reset` (no cursor,
a cursor ahead of the log, or one older than the retained window). Prompt
frames accept `request_id`; the server keeps a 64-entry FIFO and answers a
repeat with a `duplicate` frame instead of a second turn. The shipped
attach client generates request ids. Cross-restart idempotency needs a
durable submission table and is future work.

## Paths

- `crates/glass/src/{session,partial,agent,serve}.rs`, `config.rs`, `main.rs`
  (`configure_fsync`, `mod partial`, `with_live` at the headless/repl/serve/acp
  call sites).
- `scripts/smoke-recovery.py` (new), `scripts/smoke-serve.py` (extended).

## Verification

- Unit tests: repair classification (torn tail, complete corruption,
  unknown-kind passthrough, idempotent re-open), interrupted-call repair
  and its pairing, replay-policy wording, partial throttle/force/tail/
  stale-recovery, cursor math incl. the retained-window clamp, config
  validation.
- `scripts/smoke-recovery.py`: SIGKILL mid-tool; on resume the next request
  carries the interrupted result and the recovered partial; the command is
  not re-run (marker count stays 1); the sidecar is consumed; a torn tail is
  repaired with a stderr notice and the log parses line by line afterwards.
- `scripts/smoke-serve.py`: incremental reattach with a cursor, reset for a
  stale cursor, duplicate `request_id` runs once.

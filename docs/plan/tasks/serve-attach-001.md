---
id: serve-attach-001
scope: crates/glass serve module, gc serve / gc attach subcommands
status: done
depends-on: []
---

## Objective

A session that outlives the window: `glass-core serve [ID] [--socket PATH]`
owns the session loop and speaks newline-delimited JSON over a 0600 Unix
socket; `glass-core attach [--socket PATH]` is a terminal client that
replays, streams, prompts, and cancels, and can detach without stopping the
server. Ported from Crush's shared workspaces, `claude attach`, and Amp's
threads (research/harness-features-2026.md).

## Context

- One attached client at a time; a new connection replaces the previous one
  and receives history replay (last 200 user/assistant/tool entries from the
  raw log, so compaction does not hide the record).
- The server owns everything the interactive session owns: tools, extensions,
  checkpoints, ledger, trust gate, sandbox policy. Cancellation rides the
  existing steer path (empty steer) so owned process groups stop; SIGTERM
  ends the loop through the signal guard.
- v1 limits, documented: no remote modals (slash commands stay local), a
  single-row composer viewport in `attach`, Unix socket only (loopback by
  construction).

## Paths

- `crates/glass/src/serve.rs` (new): socket server, client demux, replay
  projection, turn pump, attach client with Editor + key reader.
- `crates/glass/src/main.rs`: `serve`/`attach` dispatch, help text.
- `scripts/smoke-serve.py`: raw-socket protocol checks plus a PTY run of the
  attach client.

## Verification

- `scripts/smoke-serve.py`: empty replay then prompt round-trip; second
  client replaces the first and sees history; `cancel` ends a stalled turn as
  `cancelled`; server survives disconnects and keeps serving; PTY attach
  submits a prompt and sees the reply.
- `cargo test -p glass-core` and Clippy.
- Manual: attach from a second terminal over ssh/tmux.

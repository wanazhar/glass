---
id: session-comms-001
scope: crates/glass serve multi-client, inbox module
status: done
depends-on: [serve-attach, loops-001]
---
# Session comms

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

Two mid-wave ports from the delta survey: multi-client attach (Crush's
two-TUIs-on-one-session is the reference; Claude Remote Control and Codex's
agents dashboard generalise it) and cross-session messaging (Claude
SendMessage/ListAgents), shipped together because both are "one session,
several participants" plumbing.

## Context

**Multi-client attach.** `serve.rs` held one `Option<Client>` and a new
connection evicted the old. Now `Vec<Client>` (cap 4): attach accumulates,
each client receives its own replay, turn output and `done`/`error` frames
broadcast, any client may cancel, mid-turn input is collected from all
clients, and a socket that stops accepting bytes is dropped instead of
stalling the session. Auth is unchanged and stated plainly: a 0600 Unix
socket, same user, no TCP listener, no token scheme to get wrong.

**Session inbox.** `inbox.rs`: `<data>/inbox/<id>.jsonl` (0600), one JSON
object per line. `gc send <id|--latest> <msg>` and the in-session
`/send <id> <msg>` append; a running session's idle path (repl wait loop and
serve idle branch) takes one message per tick and injects it as `System`
note + user turn. The reader advances only on complete, parseable lines —
a writer mid-append is never consumed, and a malformed complete line is
skipped, not retried forever. 32 KB per message, ids validated as filenames.

## Paths

- `crates/glass/src/serve.rs` (clients vec, broadcast, idle branch),
  `inbox.rs` (new, with tests), `main.rs` (dispatch, `/send`, help, palette).
- `scripts/smoke-serve.py` (broadcast/cancel-from-any-client cases),
  `scripts/smoke-inbox.py`.

## Verification

- Unit tests: mailbox round-trip, one-at-a-time delivery, partial-line
  race, oversize/empty/bad-target refusals, id rules.
- `scripts/smoke-serve.py`: two clients both receive a turn; cancel from
  the non-requesting client; late attach still replays; PTY attach.
- `scripts/smoke-inbox.py`: idle PTY session receives `gc send`, runs a turn
  carrying the message and provenance; invalid target refused.

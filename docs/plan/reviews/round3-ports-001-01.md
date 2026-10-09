---
id: round3-ports-001-01
scope: independent verification of the round-3 borrow list
status: done
depends-on: [round3-ports-001]
---

## What was verified

The round-3 port list in [the task](../tasks/round3-ports-001.md): the subagent
report envelope, the frozen declared tool list, the compaction system-prompt
checkpoint, MCP `tools/list` determinism and `ttlMs`, OSC 777, `/name`, the
serve lifecycle commands, extension durable entries, `/skill-doctor`, and
`/code-review`.

## Evidence

- Workspace tests: 517 passed, 0 failed (433 `glass-core`, 73 `glass-tui`, 11
  `glass-mod-api`). New unit cases cover: the envelope frame; the frozen list
  surviving a mid-session extension disable across two agents (the REPL builds
  one per turn) plus the one-time transcript note; a compaction epoch
  checkpointing the system prompt in effect (triggered with a 60-token mock
  window); `Record::Custom` and the checkpoint round-tripping through the log
  format and staying out of `to_message`; palette ranking where `/review` and
  `/code-review` no longer shadow each other.
- Strict Clippy: clean (`cargo clippy --workspace --all-targets --locked -- -D warnings`).
- Fresh release build: `cargo build --release --locked`.
- All 32 offline smoke suites plus the clean install/upgrade suite passed,
  including new coverage:
  - `smoke-mcp.py`: a fixture with `ttlMs: 0` that drops a tool on its second
    `tools/list`; after a search the dropped tool is unsearchable and the
    surviving tools still are, while the declared manifest stays the two
    meta-tools.
  - `smoke-serve.py`: `logs` reads the durable log; `stop` is acknowledged,
    the server exits and its socket disappears; `respawn` brings the same
    session back on the same socket path, attachable, and the detached server
    is cleaned up by a second `stop`.
  - `smoke-round3.py` (PTY): `/name Round Three` then `glass-core list` shows
    the title; the `/name` modal saves what you type; `/skill-doctor` reports
    both skills and the catalogue cost; `/code-review` posts the read-only
    reviewer's finding and the review prompt reaches the provider; a clean
    tree is refused without a provider call; a normal turn still runs after.

## Notes and limitations

- The envelope is a framing convention, not an escaping layer: a report that
  contains the literal closing tag is not neutralised. Its purpose is to mark
  delegated text as delegated, which it does.
- The frozen tool list is verified with a mock provider asserting the tool
  names each request carried; the cache-hit consequence itself is not
  measurable offline.
- `/skill-doctor` reports size and age, not usage: skill reads happen through
  `bash` and are not instrumented, so a last-used column does not exist.
- `/code-review` blocks the interactive loop while the delegated agent runs,
  as `/oracle` does.

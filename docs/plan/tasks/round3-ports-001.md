---
id: round3-ports-001
scope: round-3 harness borrow list
status: done
depends-on: []
---

## Objective

Ship the round-3 borrow list: small stability and safety ports plus four
capability ports that rounds one and two left behind. Every candidate was
checked against the current source before being taken, and four were
deliberately skipped.

## Context

Candidate pools: [the delta doc](../../../research/harness-delta-2026-10.md)
items marked later, pi features round two declined
([pi-durability-2026-10.md](../../../research/pi-durability-2026-10.md) and
the session-format study), and Claude Code's 2.1.x changelog. The study, the
verified gaps, and the skip decisions are recorded in
[round3-candidates-2026-10.md](../../../research/round3-candidates-2026-10.md).

## Paths

Safety and stability:

- Subagent reports are framed in the host reply (`<subagent-report>`), so
  report prose cannot read as session instructions. The frame is applied in
  `ext/host.rs` where the task labels live, so every consumer gets it.
- `Registry::declared_tools` freezes the declared extension tool list after
  the first turn: it is part of the cacheable prefix, so a mid-session
  enable/disable is noted once in the transcript (`tool set frozen …`) and
  applies to the next session. The cache warmer uses the same frozen list.
- `Record::Compacted` gains a `system` checkpoint, captured at each automatic
  compaction site in `agent.rs`; requests use it when present
  (`effective_system`), so a session resumed under changed prompt config
  replays the compacted region under the prompt that shaped it until the next
  compaction re-captures. The manual `/compact` path records none (it has no
  assembled prompt). `/context` reports the checkpoint size.

Small ports:

- The MCP bridge sorts `tools/list` output deterministically and honours
  `ttlMs`/`cacheScope` on the deferred index only (CacheableResult,
  2026-07-28); the non-deferred manifest is the frozen tool list and stays
  fixed.
- OSC 777 (`notify = "osc777"`; auto on Ghostty and WezTerm) joins OSC 9 and
  the bell.
- `/name <title>` and a prefilled modal set the session title; `glass-core
  list` shows titles.
- The command palette ranks exact and prefix matches above bare substrings,
  so `/review` and `/code-review` cannot shadow each other.

Capabilities:

- `gc stop` / `gc respawn` / `gc logs` for served sessions: `stop` is a socket
  frame (a running turn finishes first, then the socket is removed),
  `respawn` stops and restarts the server detached in its own process group,
  `logs` renders the last 40 recorded entries from the durable log.
- Extension durable entries: `Record::Custom` in the log, `append_custom` /
  `read_custom` host calls, and a startup `seed_custom`; kinds are namespaced
  by manifest name, entries never enter the model context, the agent drains
  queued appends after the tool call or turn that queued them, and exports
  show them.
- `/skill-doctor`: per-skill body bytes, catalogue bytes, and last-modified —
  honest size and age, since skill reads are not instrumented.
- `/code-review [max]`: the working diff reviewed through a read-only
  delegated agent; findings are posted as a system record so the model can
  act on them; a clean tree is refused before spending a provider call.

## Verification

Workspace tests (517 total across the three crates, including new cases for
the envelope, the frozen list, the checkpoint, custom records, and palette
ranking), strict Clippy (`-D warnings`), a fresh release build, and all 32
offline smoke suites plus the clean install/upgrade suite. New coverage: the
MCP TTL refresh case in `smoke-mcp.py`, the stop/logs/respawn lifecycle in
`smoke-serve.py`, and `smoke-round3.py` (PTY) for `/name` direct and modal,
`list` titles, `/skill-doctor` accounting, `/code-review` findings and
clean-tree refusal.

## Result

All ten ports shipped and verified; see
[the review](../reviews/round3-ports-001-01.md). The freeze is a visible
behaviour change: extension tools now arrive one session late, by design, and
the transcript says so. The checkpoint changes replay only when the prompt
config changed while a session was away.

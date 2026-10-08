---
id: loops-001
scope: crates/glass loops module, repl idle tick, serve idle loop
status: done
depends-on: [serve-attach]
---

## Objective

Scheduled runs (Claude `/loop`, Codex automations, Droid missions): a prompt
that fires on an interval while the session is idle, persisted across
resume, and — combined with `serve` — the autonomy story with no daemon.

## Context

- Sidecar `<session>.loops.json` (0600, `auth::secure_write`): entries of
  `{prompt, every_s, next_at, runs}`. `parse_every` handles `90s`/`30m`/`2h`
  and bare seconds; zero is refused; ≤8 KB prompt; ≤16 loops.
- `due()` fires only the first due entry, advances `next_at = now + every`
  (missed intervals skip — no thundering herd after a resumed day), bumps
  `runs`, and persists before returning.
- Repl idle tick: a due loop only fires into an **empty** composer, then
  synthesises an Enter key event, so it rides the ordinary submission path
  (echo, append, checkpoint, notifications) instead of duplicating it.
- Serve idle branch: a due loop becomes a `{"t":"prompt"}` message through
  the same prompt arm as a client prompt, announced to the client as text.
  While a turn runs the idle branch is unreachable, so loops cannot stack.
- `/loop`, `/loop <interval> <prompt>`, `/loop list`, `/loop rm <n>`,
  `/loop clear` — command-line form rather than a modal, which keeps it
  scriptable and smoke-testable.

## Paths

- `crates/glass/src/loops.rs` (new, with tests), `main.rs` (module, command,
  idle tick, help, palette), `serve.rs` (idle branch).
- `scripts/smoke-loop.py`.

## Verification

- Unit tests: interval parsing (including refusals), due-fire-once with
  missed-interval skip, add/list/rm/clear command flows.
- `scripts/smoke-loop.py`: PTY `/loop 1s say tick`; the mock endpoint counts
  ≥2 turns with no further input; list shows a run count; clear works.

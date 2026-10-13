---
id: ship-terminal-001
status: complete
depends-on: [ship-reliability-001, ship-reader-001]
scope: integrate reader and ship-next-20 items 10-14
---
# Delivery: terminal

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

Read ../ship-next-20.md. Integrate /search using shared Transcript query APIs,
then expandable tool cards, file/hunk diff browser, reconnecting attach, unified
activity dashboard and provider health. Reuse existing /details, /diff, /activity
sources and Progress Waiting/Retrying/Warning rather than parallel managers.

## Paths

main.rs, polish.rs, reader.rs, live.rs, modal.rs, serve.rs, provider/chain.rs,
agent.rs progress enrichment as necessary; new diff/activity modules if useful.

## Contracts

- Command preview and status always visible; selected tool expands full output
  and command; search covers full output. Load spill only explicitly, capped.
- Diff modal selects file and hunk; colors additions/removals, vertical and
  horizontal scroll; tiny viewport one focused row. Read-only Git diff source.
- Activity snapshots tools/subagents/jobs/loops; selection opens details or
  existing stop/cancel actions. Distinguish running and completed operations.
- Provider health distinguishes waiting, first token, text/reasoning stream,
  retry deadline, fallback and failures; redacted cause plus recovery commands.
- Attach reconnect preserves exact request ID and replay cursor; uncertain
  submissions retry using durable dedupe. Bounded cancellable backoff, no busy
  spin and no double transcript replay; connection banner distinct from provider.
- Composer draft survives all reader/modal operations and resize. Current motion
  and color system remain enabled. Additional shortcut help is discoverable.

## Verification

Actual VT PTYs at 40x24/20x8, long output and resize, expanded cards/search;
Git diff fixture with multiple files/hunks and long lines; mock provider phases
and errors; disposable serve kill/restart reconnection with one prompt execution.
Tests cover entry anchors and tool call/result identity, no text-equality IDs.

## Final verification

Completed in root source checkpoint `1890e59`; see
[final evidence](../reviews/ship-final-001-01.md). Independent review blockers were
fixed before the clean release run. All 40 smoke suites, 571 workspace tests,
strict Clippy and performance gates passed, with real PTYs at 40x24 and 20x8.

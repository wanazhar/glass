# Full-suite bounded load

Status: Complete and verified

## Contract

Exercise autonomous scheduling and resident services under concurrent load,
keep unrelated workspaces responsive, and make queue overflow observable.

## Implementation and evidence

- Nine-task DAG test: eight ready tasks plus one all-leaves integration task.
- Concurrent daemon test: a one-second kernel call in workspace A while
  workspace B completes workspace, browser, LSP, framed fixture-DAP, test, and
  process operations within a 300 ms responsiveness bound.
- Reconnectable 512-event daemon ring with explicit loss cursor metadata.
- Per-agent dropped Pi-event counter projected into snapshots and the TUI.
- Strict queue/batch/registry limits documented in
  `docs/full-suite-reliability.md`.

The deterministic browser operation is a disconnected resident state read.
The candidate additionally passed all 18 opt-in Chromium smoke scenarios with
the installed Chromium runtime, while the browser library suite verifies
bounded presentation mailboxes and phone/compact/desktop TUI layouts.

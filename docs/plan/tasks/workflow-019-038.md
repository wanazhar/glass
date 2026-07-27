---
id: workflow-019-038
scope: deterministic workflow scorecard
status: completed
depends-on: [workflow-019-037]
---

# Objective

Provide a repeatable browser-backed canary for the transactional workflow
contract instead of relying only on isolated unit tests.

# Delivered

Added a versioned workflow corpus and `workflow_scorecard` example. Its hard
gate validates completion, conditional skipping, bounded budget behavior,
effect-marker reconciliation, and terminal-proof enforcement against the local
fixture. Reports include expected and actual statuses, step states, trace
event counts, run IDs, and per-scenario latency.

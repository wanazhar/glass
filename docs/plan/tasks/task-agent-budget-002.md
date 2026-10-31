---
id: task-agent-budget-002
scope: Glass Core shared root budget admission
status: ready
depends-on: [task-agent-coordination-001]
---

# Shared root budget admission

## Objective

Implement the first reviewed coordination foundation: one parent/descendant
budget authority, admission reservation before a provider call or child spawn,
bounded cumulative accounting, and explicit unknown usage. Preserve existing
standalone budget behavior and keep new automatic coordination opt-in.

## Context

Read the [design](../analysis/task-agent-coordination.md), its P0/P1 contracts,
and the [review](../reviews/task-agent-coordination-001-01.md). The parent plan
has root implementation confirmation; this task does not authorize deployment,
live-provider calls or unlimited descendants.

## Path

Luna Secure owns Core `budget.rs`, necessary parent/child registry wiring in
`runtime.rs` and `subagent.rs`, and focused adjacent tests. Read existing public
contracts before changing signatures. Coordinate any shared type with the
inbox owner; do not edit Glass scheduler/UI in this slice.

## Acceptance

- Parent and children share the same synchronized allowance; parallel requests
  cannot all pass a settled-only preflight against the same remaining amount.
- Reserve, settle and abandon are idempotent; retries/wakes do not reset usage.
- Unknown price/usage is visible and cannot bypass a configured hard bound.
- Enforce configured turn/request/descendant limits as applicable, without
  claiming strict token limits where upstream usage is delayed or missing.
- Preserve conservative crash/uncertain reservation behavior; never rerun a
  provider request to reconstruct usage.
- No secret values or raw request bodies in accounting records.

## Verification and stopping

Use deterministic fake providers, barrier-controlled parallel admissions and
injected failures. Assert exact provider invocation counts, reservation release
and unknown-usage behavior. Run focused Core tests, formatting and strict
Clippy. Commit a reviewable checkpoint and stop for independent review; no live
provider test, service restart, install or push. One development/review cycle;
report concrete blockers instead of starting an unbounded redesign.

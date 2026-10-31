---
id: task-agent-coordination-001
scope: Glass and Glass Core task-backed coordination
status: in-progress
depends-on: []
---

# Task-backed coordination delivery plan

## Objective

Plan and review event-driven delegation tied to existing state/checklist tasks,
with bounded execution and no model polling. This record authorizes planning;
implementation follows design confirmation, not completion of this document.

## Context

- [Coordination design](../analysis/task-agent-coordination.md)
- [Existing task DAG](../../task-dag.md)
- [Core session checklist](../../../products/glass-core/docs/reference/checklist.md)
- [Core host runtime protocol](../../../products/glass-core/docs/reference/runtime.md)
- [Plan workflow](../README.md)

Source contracts to inspect include `crates/glass-core/src/todo.rs`,
`budget.rs`, `subagent.rs`, and `runtime.rs`, plus
`crates/glass-dev/src/tasks.rs`, the task tools, and the Core runtime adapter.
Record source findings in the analysis doc's P0 section; this task's P1 owner
uses them to settle the cross-package contract rather than repeat that audit.

## Path and owners

| Phase | Owner | Scope | Depends on |
|---|---|---|---|
| P0: existing-authority and gap audit | Luna Secure | Core state/todo/budget/subagent/runtime; Glass scheduler and task tools | None |
| P1: execution and inbox contract | Luna Capture | State mapping, durable events, inherited/reserved limits, cancellation, retry, wake, recovery | P0 |
| P2: host scheduler integration | Luna Secure | Deferred turn protocol, durable jobs/inbox, shared budget admission, standalone and resident adapters | P1, design confirmation |
| P3: shared state and viewer integration | Luna Acceptance as developer; separate reviewer required | Recoverable checklist binding, activity cards, read-only viewer limits/stop reasons, web parity | P2 |
| P4: independent verification | Luna Acceptance for P0–P2; root or separate verifier for its UI work | Threat/loop/recovery review and offline integration evidence | Each committed phase |
| P5: delivery | Assigned after integrated review | Matched binaries/assets, isolated installed acceptance, rollback | P4 |

Planning runs are bounded: one source audit, one draft and one review cycle per
agent, up to 20 model/tool rounds per planning assignment. If more is needed,
report the exact missing evidence and next bounded step. No automatic planning
loop, no live provider test and no release rebuild for documentation alone.

## Checklist

- [x] P0: map actual APIs and identify authorities; avoid duplicate scheduler.
- [x] P1: distinguish checklist claims from scheduler verification and define a stable cross-store task binding without assuming atomic writes.
- [x] P1: define root/child limits, reservations, unknown usage, admission, and retry accounting.
- [x] P1: define bounded event identity, redaction before publication/storage, deduplication, acknowledgement, and inbox overflow.
- [x] P1: define park-without-provider-polling, one owner-turn lease, coalesced wake caps, cancellation acknowledgement, and crash reconciliation.
- [x] P1: reject empty-evidence `Settled` verification for required tasks and account for TODO auto-continuation within the owner lease and request/turn budget.
- [x] P1: bind assignment and terminal activity cards to authenticated child/parent records and call/task identity.
- [x] Independent design review; resolve blocking findings.
- [ ] Design confirmation before implementation.
- [ ] P2: module tests plus real host delegate→wait→verified result→wake path.
- [ ] P3: existing state/todo and TUI/web views use the same execution identity.
- [ ] P4: fake-provider request-count, crash/replay and loop-exhaustion tests.
- [ ] P4: real offline TUI/web acceptance and independent review.
- [ ] P5: installed verification without restarting unrelated workers.

## Verification

Planning: verify each source pointer, explicitly record unsupported host paths,
route docs through the index and regenerate the documentation catalog. Run the
repository's wiki, coverage and depth checks. See the
[independent design review](../reviews/task-agent-coordination-001-01.md), which
records source-level implementation blockers and an explicit proposed-design
verdict, not an implementation pass.

Completing this planning checklist means only that the proposed contract has
been source-audited, reviewed, and documented. It does not mark a Core todo
item complete by implication, make a Glass execution task `succeeded`, or
declare the product root complete. Product completion requires the integrated
acceptance evidence in P2–P5, including verifier-backed required tasks and the
observed provider-call counts.

Implementation: preserve every integration path in the design. Require observed
provider-call counts, not a model statement that polling stopped. Preserve
existing unrelated backlog and runtime artifacts. No nightly changes or live
service restarts.

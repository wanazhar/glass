# Glass delivery plans

## Active post-0.1.18 roadmap

Issue #21 was delivered as a serial workflow-runtime sprint. Issues #25 and
#26 were included in the published 0.2.0 release; their remaining artifact and
cross-platform evidence is tracked by issue #28. The reliability task records
are [scenario contract](tasks/reliability-026-001.md),
[adversarial fixture](tasks/reliability-026-002.md), [certification gate](tasks/reliability-026-003.md),
and [capability/replay work](tasks/reliability-026-004.md). Issue #27 is now
active; its first foundation phase is [stable runtime platform](tasks/platform-027-001.md).

## Active plan: best-in-class agent browser


Status: Draft — requires owner approval before implementation

The active goal is to make Glass a deterministic, memory-efficient browser
control layer that humans and agents prefer over mature alternatives for local
Chrome automation.

Analysis and scorecard:

- [Best-in-class browser analysis](analysis/best-in-class-browser.md)
- [Agent browser automation design](../architecture/automation.md)

### Task order

| Order | Task | Outcome |
|---:|---|---|
| 1 | [quality-007](tasks/quality-007.md) | Task-success and resource scorecard before feature work. |
| 2 | [mcp-008](tasks/mcp-008.md) | Bounded, negotiated, cancellable MCP transport. |
| 3 | [target-009](tasks/target-009.md) | Unique locator resolution and verified hit targets. |
| 4 | [wait-010](tasks/wait-010.md) | Typed explicit wait engine. |
| 5 | [topology-011](tasks/topology-011.md) | Tabs, popups, targets, and frames. |
| 6 | [input-012](tasks/input-012.md) | Complete keyboard, pointer, form, and upload primitives. |
| 7 | [observe-013](tasks/observe-013.md) | Consistent, frame-aware, bounded observations. |
| 8 | [diagnostic-014](tasks/diagnostic-014.md) | Scoped console, network, dialog, and download evidence. |
| 9 | [visual-015](tasks/visual-015.md) | Exact viewport/full-page capture and visual verification. |
| 10 | [policy-016](tasks/policy-016.md) | Enforceable safety profiles and side-effect controls. |
| 11 | [release-017](tasks/release-017.md) | Supply-chain, fuzz, crash, and multi-platform hardening. |
| 12 | [compare-018](tasks/compare-018.md) | Final comparative task-success and efficiency gate. |
| 13 | [observe-019](tasks/observe-019.md) | Event-driven accessibility rejected against pinned Chromium semantics. |

Tasks are developed and independently reviewed in dependency order. A phase
does not advance while correctness or safety gates from an earlier task fail.

### Active plan: Glass 0.2.2 issue #29

Status: In progress — direct, phase-gated local development

The complete issue outline, dependency map, integration points, and atomic
commit boundaries are recorded in
[analysis/release-029.md](analysis/release-029.md). Phase tasks:

1. [release-029-001](tasks/release-029-001.md) — contract foundations
2. [release-029-002](tasks/release-029-002.md) — installation diagnostics
3. [release-029-003](tasks/release-029-003.md) — bounded agent operations
4. [release-029-004](tasks/release-029-004.md) — state and templates
5. [release-029-005](tasks/release-029-005.md) — maintainability and distribution
6. [release-029-006](tasks/release-029-006.md) — release verification

### Active plan: Glass Semantic Execution Engine issue #30

Status: In progress — local evidence-foundation increments

Completed:

- [ir-030-001](tasks/ir-030-001.md) — versioned fixture corpus and static
  baseline inventory.
- [ir-030-002](tasks/ir-030-002.md) — bounded extraction contracts, scopes, and
  resource budgets.
- [ir-030-003](tasks/ir-030-003.md) — evidence quality and coverage metadata.
- [ir-030-004](tasks/ir-030-004.md) — deterministic draft Web IR
  reconciliation.
- [ir-030-005](tasks/ir-030-005.md) — explicit opaque boundary graph entities.
- [ir-030-006](tasks/ir-030-006.md) — bounded region relationship evidence.
- [ir-030-007](tasks/ir-030-007.md) — fixture-derived draft graph expectations.
- [ir-030-008](tasks/ir-030-008.md) — evidence-backed form ownership edges.
- [ir-030-009](tasks/ir-030-009.md) — explicit relationship hints.

In progress:

- [ir-030-010](tasks/ir-030-010.md) — source-level relationship hint
  validation.

## Completed plan: performance overhaul

Status: Complete

The previous plan established compact observation, explicit expensive paths,
browser ownership, stable references, persistent MCP, a responsive TUI, and
baseline performance measurements. Its completed tasks remain below as the
delivery record:

1. [baseline-000](tasks/baseline-000.md)
2. [perf-001](tasks/perf-001.md)
3. [lifecycle-002](tasks/lifecycle-002.md)
4. [action-003](tasks/action-003.md)
5. [mcp-004](tasks/mcp-004.md)
6. [tui-005](tasks/tui-005.md)
7. [verify-006](tasks/verify-006.md)

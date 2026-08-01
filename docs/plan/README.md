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

### Completed plan: Glass 0.2.2 issue #29

Status: Complete — published on crates.io; follow-up work continues in the 0.2.3 development line

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

Status: In progress — the `0.2.7` verified-form execution increment is
implemented and exposed through the CLI and MCP; publication remains
approval-gated.

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
- [ir-030-010](tasks/ir-030-010.md) — source-level relationship hint
  validation.
- [ir-030-011](tasks/ir-030-011.md) — validated relationship-hint
  diagnostics.
- [ir-030-012](tasks/ir-030-012.md) — unmatched relationship-hint statuses.
- [ir-030-013](tasks/ir-030-013.md) — emitted and unmatched diagnostic
  expectations.
- [ir-030-014](tasks/ir-030-014.md) — corpus hint-diagnostic expectations.
- [ir-030-015](tasks/ir-030-015.md) — runtime custom-control hints.
- [ir-030-016](tasks/ir-030-016.md) — expanded custom-control hints.
- [ir-030-017](tasks/ir-030-017.md) — deterministic Web IR revision diffs.
- [ir-030-018](tasks/ir-030-018.md) — revision identity continuity.
- [ir-030-019](tasks/ir-030-019.md) — strict Task Protocol v1 contract.
- [ir-030-020](tasks/ir-030-020.md) — deterministic Task Protocol execution plans.
- [ir-030-021](tasks/ir-030-021.md) — typed task.compile protocol boundary.
- [ir-030-022](tasks/ir-030-022.md) — typed compiled-plan response.
- [ir-030-023](tasks/ir-030-023.md) — browser-free MCP compileTask integration.
- [ir-030-024](tasks/ir-030-024.md) — MCP compileTask client documentation.
- [ir-030-025](tasks/ir-030-025.md) — browser-free CLI task compile.
- [ir-030-026](tasks/ir-030-026.md) — typed MCP compileTask errors.
- [ir-030-027](tasks/ir-030-027.md) — compiler explanation mode.
- [ir-030-028](tasks/ir-030-028.md) — compiled-plan guard metadata.
- [ir-030-029](tasks/ir-030-029.md) — browser-free task validation.
- [ir-030-030](tasks/ir-030-030.md) — browser-free MCP task validation.
- [ir-030-031](tasks/ir-030-031.md) — Rust crate-root extraction and Web IR APIs.
- [ir-030-032](tasks/ir-030-032.md) — browser-free Web IR inspect and diff CLI.
- [ir-030-033](tasks/ir-030-033.md) — offline Web IR entity continuity classification.
- [ir-030-034](tasks/ir-030-034.md) — deterministic Web IR canonical JSON CLI output.
- [ir-030-035](tasks/ir-030-035.md) — offline Web IR validation command.
- [ir-030-036](tasks/ir-030-036.md) — browser-free MCP Web IR inspection and
  validation.
- [ir-030-037](tasks/ir-030-037.md) — browser-free MCP Web IR diff and
  continuity classification.
- [ir-030-038](tasks/ir-030-038.md) — canonical Glass protocol operations for
  Web IR revision analysis.
- [ir-030-039](tasks/ir-030-039.md) — canonical Glass protocol operations for
  Web IR inspection and validation.
- [ir-030-040](tasks/ir-030-040.md) — canonical Glass protocol operations for
  Task Protocol validation and compilation.
- [ir-030-041](tasks/ir-030-041.md) — route MCP Task Protocol tools through
  typed canonical dispatch.
- [ir-030-042](tasks/ir-030-042.md) — route MCP Web IR tools through typed
  canonical dispatch.
- [ir-030-043](tasks/ir-030-043.md) — typed canonical protocol response fixture
  coverage.
- [ir-030-044](tasks/ir-030-044.md) — typed canonical preflight error fixture
  coverage.
- [ir-030-045](tasks/ir-030-045.md) — advertise canonical Task and Web IR
  schema versions through capability negotiation.
- [ir-030-046](tasks/ir-030-046.md) — advertise Task Protocol and Web IR
  capability statuses.
- [ir-030-047](tasks/ir-030-047.md) — typed MCP Task validation and compilation
  errors.
- [ir-030-048](tasks/ir-030-048.md) — typed canonical Task compilation
  preflight error coverage.
- [ir-030-049](tasks/ir-030-049.md) — route CLI Task commands through canonical
  protocol helpers.
- [ir-030-050](tasks/ir-030-050.md) — route safe CLI Web IR projections through
  canonical protocol helpers.
- [ir-030-051](tasks/ir-030-051.md) — typed Web IR diff and continuity
  preflight error fixture coverage.
- [ir-030-052](tasks/ir-030-052.md) — expose the bounded canonical Web IR
  diff projection through an explicit offline CLI mode.
- [ir-030-053](tasks/ir-030-053.md) — harden deterministic Task Protocol
  execution-plan safety checks.
- [ir-030-054](tasks/ir-030-054.md) — enforce compatible Web IR revision
  transitions for diffs and continuity.
- [ir-030-055](tasks/ir-030-055.md) — verified form task execution boundary
  (implemented and covered in `0.2.7`).
- [ir-030-056](tasks/ir-030-056.md) — expose verified form task execution
  through CLI and MCP.
- [ir-030-057](tasks/ir-030-057.md) — execute bounded semantic region
  extraction through the guarded Task Protocol runtime.
- [ir-030-058](tasks/ir-030-058.md) — standardize typed task retry guidance
  across guarded execution outcomes.
- [ir-030-059](tasks/ir-030-059.md) — execute revision-guarded navigation
  tasks through CLI, MCP, and Rust.

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

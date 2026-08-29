# Glass delivery plans

## Current source line: Glass 0.3.13

Status: Current 0.3.13 source behavior for the 2026-08-27 release record.
The `0.3.12` release notes and migration guide are historical records for the
earlier tagged release; they are not current-source instructions. The
[documentation index](../INDEX.md#glass-documentation) is the navigation map
for this source line.

The [0.3.13 release notes](../releases/0.3.13.md) and
[0.3.13 migration guide](../migration/0.3.13.md) route the current release
contract. The [0.3.13 release evidence](../release-evidence.md#0.3.13-release-evidence)
holds exact-source records and pending fields for external validation. The
historical [0.3.12 release notes](../releases/0.3.12.md) and
[0.3.12 migration guide](../migration/0.3.12.md) retain their original
version claims.

| Current source area | Status and reference |
|---|---|
| TUI onboarding and development-suite launch | 0.3.13; [Development TUI](../architecture/development-tui.md) and [Development Runtime](../development-runtime.md) |
| Editor source/diff rendering, soft-wrap, cursor synchronization, and review state | 0.3.13; [Development TUI](../architecture/development-tui.md) and [Development Runtime](../development-runtime.md) |
| Actor-attributed editor collaboration | 0.3.13; [Development Runtime](../development-runtime.md) and [MCP tool catalog](../mcp-tools.md) |
| Kitty/live browser presentation | 0.3.13; [Mobile and remote](../mobile-remote.md) and [browser connection](../architecture/browser-connection.md) |
| Pi runtime and external harness workflow | 0.3.13 with Pi SDK 0.84.3; [Native Pi SDK runtime](../pi-sdk-runtime.md), [Development Runtime](../development-runtime.md), and [CLI](../cli.md) |

## Historical plan: Glass v0.3.6 issue #36

Status: Historical/superseded — this 0.3.6 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.13. Current-checkout
additions are listed in the current 0.3.13 source section above.

Issue [#36](https://github.com/wanazhar/glass/issues/36) is the authoritative
urgent product-repair contract. The 35-pillar baseline, locked architecture,
dependency order, scenarios A-J, and 15 release gates are mapped in
[the v0.3.6 delivery analysis](analysis/release-036.md). The serial tasks are:

1. [correctness-036-001](tasks/correctness-036-001.md) — lifecycle, operations,
   verification, Pi setup, and semantic documentation truth;
2. [browser-workspace-036-002](tasks/browser-workspace-036-002.md) — one shared
   browser controller/view and standalone/embedded parity;
3. [product-ux-036-003](tasks/product-ux-036-003.md) — Agent, Code, App,
   Terminal, Tasks, Git, Debug and contextual desktop interaction;
4. [mobile-onboarding-036-004](tasks/mobile-onboarding-036-004.md) — intentional
   phone navigation, onboarding, and searchable palette;
5. [certification-036-005](tasks/certification-036-005.md) — integrated, PTY,
   remote-boundary, package, exact-tag, publication, and release evidence.

Each completed checkpoint receives a focused conventional commit before the
next task starts.

## Historical plan: Glass v0.3.5 issue #35

Status: Historical/superseded — this 0.3.5 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.13. Current-checkout
additions are listed in the current 0.3.13 source section above.

Issue [#35](https://github.com/wanazhar/glass/issues/35) is the authoritative
sixteen-pillar trust and runtime-convergence contract. The audited baseline,
dependency order, twelve release gates, scenarios A-I, forbidden outcomes, and
disk-aware validation policy are in [the v0.3.5 delivery analysis](analysis/release-035.md).
The first release-blocking checkpoint is the
[workspace trust boundary](tasks/security-035-001.md).
The [native Pi SDK boundary](tasks/runtime-035-002.md) and
[product-boundary deletion](tasks/product-boundary-035-013.md) are complete in
the source candidate.
The [autonomous task and workspace actor checkpoint](tasks/scheduler-035-003.md)
is also complete locally and removes the global daemon workspace lock.
The [native transport checkpoint](tasks/platform-035-004.md) implements Windows
named pipes; only a remote native Windows run can certify that platform.
The [automatic experiment evidence checkpoint](tasks/experiments-035-005.md)
adds measured provenance and trusted deterministic ranking.

Delivery order:

1. workspace trust and customization authority;
2. native Pi SDK and completed package ownership migration;
3. autonomous verified task DAGs;
4. per-workspace daemon actors and native Windows IPC;
5. automatic experiments, DAP breadth, kernel bindings, graph/replay;
6. TUI parity, stress scenarios, documentation, packaging, and release gates.

Each completed checkpoint receives a focused local conventional commit before
the next checkpoint starts.

## Historical plan: Glass v0.3.4 issue #34

Status: Historical — released and publicly verified on 2026-08-10.

Issue [#34](https://github.com/wanazhar/glass/issues/34) is the authoritative
18-pillar full-agentic-development-suite contract. The baseline inventory,
dependency order, checkpoint boundaries, forbidden outcomes, and release gate
evidence map are in [the v0.3.4 delivery analysis](analysis/release-034.md).
The live audit is in the [v0.3.4 gate review](reviews/release-034-gates.md).

Delivery order:

1. ownership and TUI boundary decomposition;
2. native Pi sessions, governed resident tools, and agent scheduling;
3. shared LSP, real DAP, Git, tests, and persistent kernels;
4. durable workspace ownership, experiments, graph, and replay;
5. agent-native desktop/mobile surfaces and integrated browser/workflow tools;
6. synchronized 0.3.4 packaging, full-system demonstrations, and release gates.

Every completed delivery checkpoint is committed locally with a focused
conventional commit before the next checkpoint begins.

## Historical plan: Glass 0.3.3 documentation depth

Status: Historical — completed and verified locally as documentation-only, with
no push, tag, publication, release, or issue mutation.

The [depth audit](analysis/documentation-depth-035.md) accounts for every
public documentation surface and replaces name-presence checks with workflow,
state, limit, failure, recovery, and exact nested-command contracts. The
implementation record is
[documentation-depth-035-001](tasks/documentation-depth-035-001.md).

## Historical plan: Glass v0.3.3 issue #33

Status: Historical/superseded — this 0.3.3 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.13. Current-checkout
additions are listed in the current 0.3.13 source section above.

Issue #33 and its authoritative amendment define 15 integration pillars, 53
mandatory release checkboxes, scenarios A–K, full-suite command exposure and
zero-exit browser recovery. The complete defect baseline, integration chains,
TUI inspiration decisions and evidence matrix are in
[analysis/release-033.md](analysis/release-033.md); the auditable 53/53 mapping
is in [reviews/release-033-gates.md](reviews/release-033-gates.md).

Delivery order:

1. [presentation-033-001](tasks/presentation-033-001.md) — independent
   connection dimensions, policy matrix, observatory and correct local pacing.
2. [tui-recovery-033-002](tasks/tui-recovery-033-002.md) — phone shell,
   command palette, browser controller, recovery sheet and target picker.
3. [remote-agent-033-003](tasks/remote-agent-033-003.md) — secure Remote View
   and live attachment-aware agent context.
4. [runtime-033-004](tasks/runtime-033-004.md) — process trees, project tree,
   persistent LSP and real embedded Neovim proof.
5. [release-033-005](tasks/release-033-005.md) — full-suite binaries,
   cross-platform/tool CI, black-box demonstrations, docs and final validation.

## Historical plan: complete public documentation and docs.rs revamp

Status: Historical — completed and verified locally; no remote mutation.

The [documentation audit](analysis/documentation-revamp-034.md) and
[implementation task](tasks/documentation-034-001.md) cover every current
user, operator, SDK, MCP, TUI, package, rustdoc, example, and release-reference
surface, with generated drift checks against the implementation.

## Historical plan: semantic resource and correctness audit

Status: Historical — completed and verified locally; no remote mutation.

The [resource audit](analysis/semantic-resource-audit.md) and atomic
[implementation task](tasks/semantic-resource-033-002.md) optimize the task
compiler, live binding, agent gateway, and private Pi request boundary while
preserving the completed semantic-core contracts.

## Historical plan: semantic core hardening

Status: Historical — completed and verified locally; no remote mutation.

The [delivery analysis](analysis/semantic-core-hardening.md) and atomic
[implementation task](tasks/semantic-core-033-001.md) cover executable Web IR
evidence, relationship-scoped compilation, revision-bound execution,
capability and state fidelity, continuity, and bounded Local/Pi semantic tools.

## Historical/superseded post-0.1.18 roadmap
Status: Historical/superseded — the roadmap body records delivered 0.2.x work
and later issue tracking; it is not the current source-line plan.

Issue #21 was delivered as a serial workflow-runtime sprint. Issues #25 and
#26 were included in the published 0.2.0 release; their remaining artifact and
cross-platform evidence is tracked by issue #28. The reliability task records
are [scenario contract](tasks/reliability-026-001.md),
[adversarial fixture](tasks/reliability-026-002.md), [certification gate](tasks/reliability-026-003.md),
and [capability/replay work](tasks/reliability-026-004.md). Issue #27 is now
active; its first foundation phase is [stable runtime platform](tasks/platform-027-001.md).

## Historical plan: Glass v0.3.1 issue #31

Status: Historical/superseded — this 0.3.1 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.13. Current-checkout
additions are listed in the current 0.3.13 source section above.

The authoritative [issue #31](https://github.com/wanazhar/glass/issues/31)
defines four mandatory pillars—semantic memory, multi-surface understanding,
runtime survivability beyond CDP, and the Ratatui-native Browser Workspace—
plus the cross-cutting Glass Experience Layer. The delivery analysis,
integration inventory, and dependency order are in
[analysis/release-031.md](analysis/release-031.md).

Foundation wave:

1. [memory-031-001](tasks/memory-031-001.md) — surface/backend-aware
   knowledge provenance and explainability.
2. [surface-031-001](tasks/surface-031-001.md) — bounded multi-surface
   contract.
3. [backend-031-001](tasks/backend-031-001.md) — transport-neutral Browser
   Capability Interface.
4. [presentation-031-001](tasks/presentation-031-001.md) — bounded latest-frame
   presentation contract.
5. [workspace-031-001](tasks/workspace-031-001.md) — addressable workspace and
   mutation-lease contract.

These tasks are intentionally disjoint and do not edit shared exports or
dispatch files. Integration tasks begin only after committed implementation
and independent review.

## Historical plan: Glass v0.3.2 issue #32

Status: Historical/superseded — this 0.3.2 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.13. Current-checkout
additions are listed in the current 0.3.13 source section above.

Issue #32 is an architectural epic. The original thin-slice interpretation was
rejected during the issue/comment audit; the current candidate is reviewed
against every pillar, mandatory release gate, visual comment, and packaging
comment in the [delivery evidence matrix](analysis/release-032.md).

Delivery order:

1. [development-032-001](tasks/development-032-001.md) — project detection,
   bounded files/editor, PTY process runtime, events, graph, and diff core.
2. [development-032-002](tasks/development-032-002.md) — shared CLI and MCP
   project-runtime contracts for humans and external agents.
3. [development-032-003](tasks/development-032-003.md) — native TUI project
   surface and embedded harness interaction.
4. [release-032-001](tasks/release-032-001.md) — package boundary, versioned
   documentation, release validation, and local 0.3.2 candidate checkpoint.

The candidate must keep the v0.3.1 browser intelligence contracts intact. Any
capability that cannot provide evidence in this checkout is reported as
experimental or unavailable; it is not presented as a completed framework
integration.

Integration wave (after foundation review):

- [surface-031-002](tasks/surface-031-002.md) — integrate surfaces into Web
  IR extraction.
- [backend-031-002](tasks/backend-031-002.md) — route the CDP implementation
  through the Browser Capability Interface.
- [presentation-031-002](tasks/presentation-031-002.md) — add terminal graphics,
  bounded frame presentation, and semantic fallback.
- [workspace-031-002](tasks/workspace-031-002.md) — connect workspace identity
  to profiles, sessions, memory, and attachments.
- [memory-031-002](tasks/memory-031-002.md) — connect retrieval and provenance
  to compiler advisories.
- [experience-031-001](tasks/experience-031-001.md) — expose the shared
  Experience Layer across CLI, TUI, and MCP.
- [integration-031-001](tasks/integration-031-001.md) — run the integrated
  four-pillar conformance demonstration.

## Historical plan: remote development cockpit

Status: Historical — implemented and verified by direct serial work on the
local 0.3.2 candidate.

The post-issue-32 product enhancements are defined in the
[delivery analysis](analysis/mobile-cockpit.md) and implemented as the atomic
[mobile-cockpit-001](tasks/mobile-cockpit-001.md) task.

## Proposed plan: best-in-class agent browser

Status: Proposed draft — requires owner approval before implementation; it is
not a commitment for the current `0.3.13` source line.

The proposed goal is to make Glass a deterministic, memory-efficient browser
control layer that humans and agents prefer over mature alternatives for local
Chrome automation.

Analysis and scorecard:

- [Best-in-class browser analysis](analysis/best-in-class-browser.md)

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

### Completed plan: Glass Semantic Execution Engine issue #30

Status: Complete — `glass-browser 0.3.0` is published on crates.io and
`v0.3.0` has the matching source-only GitHub Release. The epic exit contract
and release delivery record are complete.

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
- [ir-030-060](tasks/ir-030-060.md) — execute revision-guarded semantic tab
  selection within scoped regions.
- [ir-030-061](tasks/ir-030-061.md) — execute guarded inspect, confirm, and
  cancel dialog tasks through CLI, MCP, and Rust.
- [ir-030-062](tasks/ir-030-062.md) — execute bounded revision-guarded
  pagination advances within semantic pagination regions.
- [ir-030-063](tasks/ir-030-063.md) — expose typed pending-dialog details
  through `dialog.inspect` task results.
- [ir-030-064](tasks/ir-030-064.md) — route CLI and MCP browser-backed task
  execution through one canonical Rust dispatcher.
- [ir-030-065](tasks/ir-030-065.md) — execute bounded `collection.extract`
  against uniquely scoped semantic collection regions.
- [ir-030-066](tasks/ir-030-066.md) — execute bounded `table.extract` against
  uniquely scoped semantic table regions.
- [ir-030-067](tasks/ir-030-067.md) — execute guarded `field.read` with bounded
  form-state output and policy-preserving redaction.
- [ir-030-068](tasks/ir-030-068.md) — harden `field.read` with sensitive-value
  redaction coverage and post-observation revision checks.
- [ir-030-069](tasks/ir-030-069.md) — require `inputs.field` during authored
  `field.read` validation.
- [ir-030-070](tasks/ir-030-070.md) — require explicit semantic region scopes
  for browser-backed task families.
- [ir-030-071](tasks/ir-030-071.md) — execute bounded `pagination.collect` with
  revision-aware page advances and recovery guidance.
- [ir-030-072](tasks/ir-030-072.md) — harden extraction revision checks and
  semantic no-op detection for pagination collection.
- [ir-030-073](tasks/ir-030-073.md) — add guarded `navigation.openMenu`
  execution with semantic menu-control targets.
- [ir-030-074](tasks/ir-030-074.md) — verify `navigation.openMenu` outcomes
  through observable expanded state and typed indeterminate recovery.
- [ir-030-075](tasks/ir-030-075.md) — verify `navigation.selectTab` through
  bounded ARIA-selected polling and indeterminate recovery.
- [ir-030-076](tasks/ir-030-076.md) — require a bounded semantic page or route
  transition after `pagination.next`, with delayed-success and no-op recovery
  coverage.
- [ir-030-077](tasks/ir-030-077.md) — verify `navigation.follow` reaches the
  requested destination and return indeterminate recovery for redirects or
  other URL mismatches.
- [ir-030-078](tasks/ir-030-078.md) — restrict `form.submit` to
  evidence-backed semantic button targets and fail closed for named fields or
  other non-submit controls.
- [ir-030-079](tasks/ir-030-079.md) — convert `form.fill` operation and
  post-fill inspection failures into bounded indeterminate recovery results.
- [ir-030-080](tasks/ir-030-080.md) — bound mutation verification failures
  and require explicit `form.submit` postconditions.
- [ir-030-081](tasks/ir-030-081.md) — add typed structured-extraction kinds,
  field-level provenance, and explicit output-limit metadata.
- [ir-030-082](tasks/ir-030-082.md) — add bounded item-level records for
  semantic table and repeated-collection extraction.

- [ir-030-083](tasks/ir-030-083.md) — populate bounded semantic table and
  collection records from accessibility evidence.
- [ir-030-084](tasks/ir-030-084.md) — include structured record changes in
  revision-aware semantic page checks.
- [ir-030-085](tasks/ir-030-085.md) — add bounded, revision-bound
  continuation metadata for truncated extraction.
- [ir-030-086](tasks/ir-030-086.md) — validate continuation revision and route
  before resuming extraction.
- [ir-030-087](tasks/ir-030-087.md) — bind continuations to the requested
  semantic region.
- [ir-030-088](tasks/ir-030-088.md) — bind continuations to the extraction
  field contract.
- [ir-030-089](tasks/ir-030-089.md) — add fail-closed sensitive extraction
  gating for secret-like field names and paths.

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

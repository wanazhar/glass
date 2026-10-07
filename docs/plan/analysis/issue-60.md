# Issue #60 cross-surface bug delivery analysis

Status: in progress. The canonical checklist and complete audit coverage are
maintained in [GitHub issue #60](https://github.com/wanazhar/glass/issues/60).

## Scope and contract

Issue #60 tracks findings F1–F100 from reports #41–#50, plus standalone daemon,
protocol, and connection reports #52–#59. Reports #41–#59 are closed as
superseded; that disposition does not prove their reported code paths are
fixed. Issue #40 is a separate browser-engine feature epic and is excluded.

Use each issue’s reproduction and suggested remedy together with the current
source tree. Preserve the priority and definition of done in #60: resolve P0
policy/data-integrity issues first, then P1 reliability and cross-entry-point
contracts, then P2/P3 behavior. Keep the tracker open while any P0/P1 finding
remains. Mark a finding complete only after a focused regression or an explicit
reproducible verification establishes the contract across the affected entry
points.

F92’s schema metadata placement is implemented in `53fd2ca9`. Its focused
schema regression passed and is recorded in the
[governance task](../tasks/issue60-p0-governance-001.md). F3’s unified TUI
command/action routing and visual-runtime reconciliation passed independent
review; evidence is recorded in the
[command-routing task](../tasks/issue60-p1-tui-command-routing-001.md) and its
[review report](../reviews/issue60-p1-tui-command-routing-001-01.md).

## Module and integration map

| Boundary | Owners | Integration path to inspect |
|---|---|---|
| Trust and mutation authority | `glass-dev` workspace, customization, tools, CLI, MCP, kernels, external agents | Workspace trust and effective mutation descriptors must agree with enforcement for direct library, CLI, TUI, MCP, daemon, and harness calls. |
| Browser policy and revision | `glass-browser` policy, Chrome launch, sessions and interception; `glass-dev` resident browser service and TUI | CLI policy options flow through resident sessions into the browser workspace, launch arguments, request interception, revision guards, and rendered capability state. |
| TUI input and state | `glass-dev/src/tui` plus editor, agent, browser, and snapshot workers | Keyboard/mouse route through one active overlay/surface; asynchronous results stay attached to their originating request and visible state. |
| Editor and collaboration integrity | development project buffers, collaboration claims, snapshots, undo/cache/dirty metadata | Every edit, selection replacement, checkpoint restore, claim, and release must publish consistent revision and state changes. |
| Daemon lifecycle and transport | daemon operation registry, workspace actor, stream framing, startup | Request, cancellation, worker completion, workspace close, response encoding, and process startup must have recoverable terminal states. |
| CLI/MCP/harness contracts | Clap commands, MCP descriptors/adapters, external harness launch | Advertised schemas/capabilities must match execution requirements, bounds, stdio ownership, and confirmation behavior. |

The cross-boundary flows above are the integration tasks; isolated helper
changes are insufficient where the issue names multiple entry points.

## Delivery order

1. **P0 policy, trust, mutation, and security.** Audit and repair effective
   trust reporting, mutation confirmation, host policy forwarding, browser
   denial precedence, resolver argument order, target query privacy, kernel
   and protected-branch contracts, protocol identifiers, and library sandbox
   confirmation.
2. **P0 data preservation and revisions.** Repair TUI exit/FIM/chat state,
   editor claims and checkpoint/selection mutation, and daemon completion
   races. Completed work is recorded in the linked task documents in the plan
   index, including the focused source report #55 daemon result-race task.
3. **P1 lifecycle and transport.** Repair daemon locks, process startup,
   workspace actor recovery/close, and bounded response handling; then browser
   and worker lifecycle issues. The first bounded slice covers reports #52–#54,
   #56, and #59 in
   [the daemon lifecycle task](../tasks/issue60-p1-daemon-lifecycle-001.md).
4. **P1 TUI, editor, browser, workspace, and MCP reliability.** The completed
   overlay-priority and unified command-routing slices are recorded in the
   [TUI overlay task](../tasks/issue60-p1-tui-overlay-priority-001.md) and
   [TUI command-routing task](../tasks/issue60-p1-tui-command-routing-001.md).
   Continue with pointer hit geometry in the
   [F5 task](../tasks/issue60-p1-tui-pointer-geometry-001.md), then resolve
   stale revisions, orphaned jobs, snapshot state, editor metadata, browser
   identity, schemas, and persistent option forwarding. F7's selection-preview
   ownership and latest-wins behavior is scoped in the
   [worker request-coalescing task](../tasks/issue60-p1-worker-request-coalescing-001.md).
5. **P2 functional/UX contracts, then P3 consistency.** Align surfaces,
   layout/pointer behavior, help, CLI aliases, and low-risk presentation.

## Validation and delivery constraints

For every P0/P1 task, record the affected entry points, contract decision,
focused regression coverage or reproducible command sequence, and any remaining
platform gap. TUI fixes require interaction-level evidence. Cross-surface fixes
must exercise the real boundary or record why a real path is unavailable.

Use isolated task branches/worktrees for development and a different reviewer
for verification, following the delivery-plan workflow. Merge each accepted
task into `main`, push focused Conventional Commits, and remove temporary task
branches/worktrees after integration. No release, tag, or publication is in
scope.

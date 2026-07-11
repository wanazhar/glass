# Best-in-class agent browser analysis

Status: Draft

## Objective

Turn Glass from a fast basic CDP driver into a browser-control layer that wins
on correctness per byte: high task success, zero silent wrong-target actions,
compact agent context, low steady-state memory, low latency, and clear safety
boundaries.

## Definition of best in class

Glass is best in class only when it wins a published, reproducible scorecard.
Feature count and microbenchmark latency alone do not qualify.

| Dimension | Initial gate | Measurement |
|---|---:|---|
| Wrong-target actions | 0 | Adversarial duplicate/overlay/reflow suite. |
| Deterministic fixture task success | 100% | Repeated local scenarios, 100 runs each. |
| Representative task success | >=95% | Versioned public workflow corpus. |
| Fresh compact observe p95 | <=5 ms | Warm local fixture, excluding Chrome startup. |
| Cached compact observe p95 | <=0.1 ms | Same session and revision. |
| Fast action client overhead p95 | <=5 ms | Excludes page-authored reaction time. |
| Idle Glass RSS | <=8 MiB | Excludes Chrome and allocator warm-up variance. |
| Peak default-workflow Glass RSS | <=16 MiB | Navigate/observe/act loop, no image/deep DOM. |
| Compact context p95 | <=24 KiB | Serialized agent-facing payload. |
| Release binary | <=6 MiB | Stripped default release profile. |
| MCP malformed-input survival | 100% | Fuzz corpus plus size/time-limit cases. |

These are product budgets, not promises about Chrome memory or network/page
latency. Task `quality-007` records hardware and ratifies thresholds before
feature implementation. Any changed threshold requires an explanation and
before/after evidence in the plan.

### Ratified baseline: 2026-07-11

The initial gates remain unchanged. A three-iteration optimized local run of
`glass-local-v1` on Linux aarch64, Rust 1.97.0, and Chromium 150 recorded 15/33
correct outcomes (45.5%), three wrong-target actions, twelve explicitly
unsupported outcomes, and three ordinary delayed-content failures. This is an
intentional honest baseline: the duplicate-label scenario proves the harness
catches the unsafe first-substring behavior assigned to `target-009`, while
future-capability scenarios no longer simulate success through JavaScript.

The same run measured 6,029,312 bytes peak post-startup workflow Glass RSS,
1,061,949,440 bytes peak Chrome process-tree RSS, 11,476 compact context bytes, and a 4,398,992-byte
release binary. The disjoint process scopes make the large Chrome figure
explicit rather than charging it to Glass. The existing operation benchmark
recorded fresh compact observation p95 3.54 ms and cached observation p95 0.02
ms. Fast click p95 was 17.15 ms end to end; the <=5 ms client-overhead gate is
retained but requires the action instrumentation delivered with `target-009`
to separate client work from Chrome and page reaction time.

These samples establish that the memory, compact-context, observation, and
binary gates are realistic on the recorded host. They do not waive the zero
wrong-action or 100% deterministic-success gates. Release comparisons use 100
iterations; these shorter measurements ratify the thresholds without claiming
statistical release evidence.

## Module decomposition

| Module | Inputs | Outputs | Dependencies | Delivery task |
|---|---|---|---|---|
| Scorecard harness | Scenario corpus, competitors | Success/resource report | Browser fixture, process metrics | `quality-007` |
| MCP transport | Framed JSON-RPC | Bounded typed requests/responses | Tokio I/O | `mcp-008` |
| Locator resolver | Locator + target/frame/revision | Unique/ambiguous/not-found | AX, DOM, hit testing | `target-009` |
| Wait engine | Typed condition + deadline | Satisfied/timeout/cancelled | CDP events, bounded polling | `wait-010` |
| Topology registry | Target/frame events | Explicit target/frame handles | CDP target sessions | `topology-011` |
| Input engine | Typed action + resolved target | Action outcome | Locator, topology, CDP Input | `input-012` |
| Observation engine | Target/frame + budget | Consistent compact context | Runtime, AX, DOM | `observe-013` |
| Diagnostics | Scoped subscriptions | Bounded evidence | Console, Network, Page | `diagnostic-014` |
| Visual engine | Capture request | Image metadata/payload/diff | Page, Emulation, screencast | `visual-015` |
| Policy engine | Operation + context | allow/deny/confirm | Frontends, session | `policy-016` |
| Release system | Source/tag/platform | Signed verified artifacts | CI, Cargo, Chrome | `release-017` |

## Integration enumeration

Every chain below needs at least one real-Chrome test; module-only mocks are not
sufficient.

1. CLI/MCP/TUI creates session with policy and resource budgets.
2. Session launches or attaches Chrome and creates the topology registry.
3. Registry attaches CDP sessions to explicit targets and frames.
4. Observation resolves topology, collects Runtime + AX state, and publishes
   revisioned references.
5. Locator consumes those references or an explicit locator and returns a
   unique target or an ambiguity error.
6. Wait engine observes target/frame lifecycle and resolves a typed condition.
7. Input engine revalidates the resolved target, dispatches input, invalidates
   observation, and returns evidence.
8. Diagnostic scopes enable domains for their lifetime and disable them when
   the last subscriber leaves.
9. Visual scopes capture or stream without entering the default observation
   cache.
10. Policy intercepts navigation, evaluation, upload/download, persistent
    profile, and filesystem boundaries before side effects.
11. MCP cancellation reaches waits and pending CDP calls without corrupting the
    session.
12. Scorecard drives identical workflows through Glass and comparison adapters.

## Delivery strategy

### Phase 0: establish truth

Build the task corpus and resource harness first. Include success, wrong-action,
timeout quality, context bytes, allocations/RSS, latency, and binary size.
Retain raw run metadata and compare medians across repeated runs.

### Phase 1: remove unsafe failure modes

Bound MCP input, negotiate protocol versions, reject ambiguous locators, verify
hit targets, and add explicit waits. This phase is required before adding more
side-effecting actions.

### Phase 2: establish browser topology

Represent pages, popups, and frames explicitly. Avoid a single global "current
page" that changes behind an agent. Route commands through typed target/frame
handles while retaining a convenient default for one-page sessions.

### Phase 3: complete interaction and observation

Add missing input primitives and frame-aware observation. Prefer CDP-native
actions, stable handles, event-driven waits, and one-pass projection. Keep deep
DOM and images explicit.

### Phase 4: add evidence on demand

Add scoped console, network, dialog, download, and visual capabilities. Domain
activation follows subscription lifetime so idle/default cost remains near
zero.

### Phase 5: policy and production hardening

Make dangerous operations enforceably configurable, harden browser supply
chain and crash cleanup, fuzz protocol/parser boundaries, and test all claimed
platforms.

### Phase 6: earn the claim

Publish the scenario definitions and comparison methodology. Release only if
Glass meets correctness gates and demonstrates a meaningful efficiency win.

## Architectural guardrails

- Do not add Playwright, WebDriver, an embedded browser, or a general plugin
  runtime to the browser data plane.
- Prefer typed structs over pervasive `serde_json::Value` beyond the CDP edge.
- Use bounded `mpsc`, watch channels, or single-owner state; avoid cloning large
  event payloads through broadcast.
- Make expensive CDP domains lease-based and opt-in.
- Parse large payloads once and move or stream them.
- Do not cache screenshots, full DOM, network bodies, or traces by default.
- Do not simulate human behavior as a stealth claim. Human mode exists for
  inspectability and realistic interaction timing.
- A convenience API may compose primitives but may not hide unbounded waits,
  ambiguous selection, or silent fallback.

## Competitive method

Compare Glass with at least Playwright and one agent-focused browser MCP on the
same Chrome build, machine, profile state, viewport, and fixtures. Codex's
built-in browser can be compared only as a black-box task runner where the
surface is available; do not claim internal implementation advantages.

Score outcomes in this order:

1. correct completion;
2. wrong or unsafe action;
3. recovery quality;
4. agent input/output bytes;
5. wall time and CDP round trips;
6. Glass/runner RSS and peak memory; and
7. installation and binary footprint.

## Principal risks

| Risk | Mitigation |
|---|---|
| Capability growth destroys size/memory advantage | Per-task resource delta gate and opt-in domains. |
| Event-driven state becomes inconsistent | Single-owner topology state and revisioned snapshots. |
| "Human" becomes an anti-bot promise | Explicitly reject stealth positioning. |
| Benchmark overfits local fixtures | Versioned public scenarios plus external-site canaries that are not release blockers. |
| MCP evolution breaks clients | Negotiation and compatibility fixtures for supported protocol versions. |
| Cross-platform claims outrun testing | CI matrix and artifact smoke test before release. |
| Safety policy harms usability | Development and hardened presets with typed denial explanations. |

## Exit criteria

The program completes only when `compare-018` passes every correctness and
safety gate, publishes resource deltas against the ratified baseline, and has
no blocking review findings. A fast partial implementation remains a useful
release, but it is not labeled best in class.

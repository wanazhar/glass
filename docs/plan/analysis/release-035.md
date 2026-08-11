# Glass v0.3.5 delivery analysis

Status: Active direct implementation. Issue
[#35](https://github.com/wanazhar/glass/issues/35) is authoritative. The issue
had no comments or timeline amendments when this analysis was recorded on
2026-08-11. No push, tag, publication, GitHub Release, or issue mutation is
authorized by this plan.

## Release thesis and locked decisions

Glass Dev must converge into one trustworthy autonomous development runtime.
Pi remains its only embedded agent runtime; OMP remains an external benchmark.
`glass-dev` owns development state and depends one way on the browser-only
`glass-browser` package. Structured browser observation, explicit screenshots,
bounded queues, resident services, mobile operation, and ownership semantics
from 0.3.3/0.3.4 must not regress.

## Audited baseline

The 0.3.4 checkout proves real resident browser, LSP, DAP, test, Git, kernel,
experiment, Pi, TUI, MCP, and daemon implementations. The issue's material
findings are reproducible in the source:

- `DevelopmentWorkspace::open` runs `workspace.opened` before any trust
  decision and installs project skills as Glass authority.
- Shell-backed project tools can self-report `mutating = false` and remain
  available without a workspace trust gate.
- `glass-dev` enables `glass-browser/development-runtime`, embeds the
  browser-owned `ProjectWorkspace`, and falls back to browser CLI dispatch.
- Pi sessions use the browser-owned `PiHarness` and `pi --mode rpc` protocol.
- Agent dependency scheduling exists, but tasks are still agent metadata and
  settled responses return agents to idle without a verification state.
- `glassd` retains `DevelopmentWorkspace` values inside one registry mutex and
  holds it across tool execution; its durable transport is Unix-only.
- Experiment ranking accepts a largely caller-populated evidence object.
- DAP has real debugpy evidence but no reverse-request dispatcher.
- Kernels are persistent runtimes without governed tool-router bindings.
- TUI surfaces do not include first-class trust, task, daemon/workspace, or
  kernel views and ordinary trust decisions cannot be completed there.
- Release notes/signing/publication evidence need an exact-tag/publication
  truth model rather than post-tag edits to immutable evidence.

The baseline `cargo test -p glass-dev --lib` passes 37 tests. The checkout was
clean and equal to `origin/main` at `cdb8a9235084e869b66ceffc64990c406876420d`.

## Delivery dependency order

1. Establish workspace trust before any further executable project feature is
   expanded. Gate configuration, skills, hooks, tools, tests, LSP, DAP,
   experiments, daemon reconnect, CLI, MCP, Pi, and TUI through one state.
2. Move development contracts and Pi ownership into `glass-dev`; remove the
   browser feature seam and general dispatch fallback.
3. Replace `PiHarness` with a Glass-owned Pi SDK runtime process boundary and
   migrate persisted sessions explicitly.
4. Introduce session/agent/task separation, automatic dependency dispatch,
   verification, retry/cancel/pause/reassign, and public task operations.
5. Make daemon registry entries independent workspace actors and add native
   Unix/Windows local transport implementations plus reconnect events.
6. Add automatic experiment evidence providers, DAP reverse requests and real
   adapter certification, governed kernel bindings, and graph/replay links.
7. Complete desktop/compact/mobile TUI parity and full-suite stress coverage.
8. Synchronize 0.3.5 docs, packages, migration, release notes, signing truth,
   exact-tag evidence generation, fuzz/security gates, and install/upgrade
   proofs.

## Requirement and evidence matrix

| Issue pillar / gate | Required implementation evidence | Initial state |
|---|---|---|
| I / Gate 1 Trust | fail-closed identity-bound trust store, inspection, shared enforcement, ten required security tests, trust TUI | Complete locally |
| II / Gate 2 Pi SDK | Glass-owned runtime and direct session lifecycle/events/tools/persistence; no CLI-RPC harness | Complete in the source candidate; browser compatibility core deleted |
| III / Gate 3 Boundary | no browser development feature, no browser-owned project core, no fallback dispatch; three package checks | Complete in the source candidate; browser development module and legacy CLI/MCP/TUI ownership deleted |
| IV / Gate 4 Tasks | first-class verified DAG, automatic dispatch/wakeup/failure propagation, public and TUI controls | Complete locally |
| V / Gate 5 Daemon | registry-only locking, bounded per-workspace actors, reconnect/event stress proof | Complete locally; cursor reconnect and observable overflow covered |
| VI / Gate 6 Platforms | Unix socket and native Windows named-pipe lifecycle with native tests | Implemented; native Windows CI evidence pending remote run |
| VII / Gate 7 Experiments | measured providers and per-metric provenance; deterministic trusted weights | Complete locally; browser metrics explicit when unavailable |
| VIII / Gate 8 DAP | reverse requests through owned processes and three honest real-adapter evidence families | Complete locally; native three-family CI evidence pending remote run |
| IX / Gate 9 Kernels | router-mediated, attributed, cancellable, bounded capability binding; no sandbox claim | Complete locally |
| X Governance | source authority and exact executable inspection for skills/hooks/tools/commands | Complete locally |
| XI Graph/replay | typed causal links across every listed subsystem; observable evidence only | Complete locally |
| XII / Gate 10 TUI | all listed surfaces and lifecycle/recovery actions on desktop, compact, phone | Complete locally; deterministic buffer coverage at all three geometries |
| XIII parity | one router plus task/trust APIs across CLI/MCP/daemon/Pi | Complete locally; daemon tools and bounded event cursor retain router authority |
| XIV / Gate 11 load | 8+ tasks and concurrent browser/LSP/DAP/test/workspace/reconnect bounded stress evidence | Deterministic suite complete; live Chromium presentation evidence pending final E2E |
| XV / Gate 12 release | substantive generated notes, truthful verification, immutable tag/publication record, packages/CI/fuzz/install | Workflow/body complete; maintainer key enrollment and exact-tag/package gates pending |
| XVI docs | all twelve issue topics and honest certification/migration language | Complete locally; final exact-tag evidence refresh pending |
| Scenarios A-I | executable integrated demonstrations recorded at exact candidate commit | Blocking |
| Forbidden outcomes 1-20 | explicit final audit with source/evidence pointers | Blocking |

## Validation policy

Focused tests run at each checkpoint. The final candidate requires formatting,
strict Clippy, all workspace tests, browser-free package checks, package dry
runs, isolated install smoke, fuzz/security checks, native platform jobs,
integrated scenarios A-I, and disk-space checks before heavyweight builds.
Browser screenshots remain explicit. A fixture or cross-compile is never
reported as native platform/adapter certification.

The repository currently has 28 GiB free, with 21 GiB in the reusable root
Cargo target and 5.2 GiB in the reusable fuzz target. One target tree is reused;
space is rechecked before release profiles, packages, fuzz, or cross-target
builds. Cleanup, if needed, is limited to explicitly inspected disposable Cargo
artifacts.

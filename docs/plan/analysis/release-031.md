# Glass v0.3.1 — issue #31 delivery analysis

Status: Implemented and integrated in the final local `0.3.1` release checkout.
Publication and remote release operations remain explicit maintainer actions.

Source of truth: [issue #31](https://github.com/wanazhar/glass/issues/31) and its authoritative amendments. The release contract has four mandatory pillars plus the cross-cutting Glass Experience Layer:

1. Semantic Memory and Knowledge Graph.
2. Multi-Surface Semantic Understanding.
3. Runtime Survivability Beyond CDP.
4. Terminal-Native Browser Workspace.
5. Addressable, composable, discoverable product experience.

## Baseline

`v0.3.0` already provides bounded Web IR, deterministic Task Protocol compilation, guarded workflow execution, verification/recovery, a local knowledge record/store, CDP-backed `BrowserSession`, and a Ratatui TUI. The existing contracts are authoritative where issue #31 extends them; duplicate knowledge, session, or TUI abstractions are prohibited.

Primary existing integration points:

```text
src/browser/session/knowledge.rs       persistent knowledge contract
src/browser/session/knowledge_store.rs crash-safe local knowledge store
src/web_ir.rs                          stable Web IR v1
src/task_compiler.rs                   deterministic task compilation
src/browser/session/                     BrowserSession and CDP execution
src/browser/cdp.rs                      transport implementation
src/tui/app.rs                          Ratatui worker and command loop
src/cli/args.rs + runner.rs             CLI surface and dispatch
src/mcp/server.rs + resources.rs        MCP tools/resources
src/daemon.rs                           persistent local coordinator boundary
```

## Constitutional invariants

- Fresh current Web IR is authoritative; memory is advisory only.
- Memory-assisted candidates require current validation before executable output.
- Ambiguity, unsupported capability, stale revision, and opaque surfaces fail closed with typed errors.
- Stable semantic, workflow, memory, surface, presentation, and workspace contracts contain no CDP node/target/domain types.
- Browser presentation and human input address the same `BrowserSession` target used by agents.
- At most one mutating actor owns a target lease; observers are read-only unless explicitly granted a lease.
- Frame delivery is paint-aware and bounded: latest rendered frame plus at most one newest pending frame.
- Browser zoom and capture scale are separate; active interaction targets 30–60 fps, with measured degradation.
- Memory is profile/workspace/origin scoped, redacted, inspectable, exportable, and deletable; private sessions do not learn by default.
- No Electron, OpenTUI, embedded browser engine, mandatory cloud store, or mandatory embedding provider enters the crates.io path.

## Delivery decomposition

### Foundation wave (parallel, disjoint paths)

| Task | Responsibility | Depends on |
|---|---|---|
| `memory-031-001` | Extend existing knowledge records with surface/backend portability, influence, and explainability metadata. | none |
| `surface-031-001` | Define bounded surface kinds, capabilities, coverage levels, provenance, and extension namespaces. | none |
| `backend-031-001` | Define transport-neutral backend capabilities, profiles, selection, and typed errors. | none |
| `presentation-031-001` | Define browser frames, viewport geometry, latest-frame mailbox, ownership, and presentation metrics. | none |
| `workspace-031-001` | Define stable workspace/resource references, lifecycle, actor roles, and mutation lease state. | none |

Foundation tasks must not edit `src/lib.rs` or shared dispatch files. A later integration task owns public exports and cross-module wiring, preventing parallel merge conflicts.

### Integration wave (dependency-ordered)

1. Integrate `Surface` into Web IR extraction and serialization without regressing HTML/AX fixtures.
2. Adapt the current CDP `BrowserSession` behind `BrowserBackend`; preserve CDP as production backend.
3. Add a non-CDP proof backend boundary and capability-shock failure paths.
4. Connect workspace identity and leases to daemon/session/TUI ownership.
5. Add presentation/terminal graphics contracts, latest-frame scheduling, semantic fallback, and TUI workspace modes.
6. Connect memory retrieval/reranking and provenance to compiler advisories and current Web IR validation.
7. Add Experience Layer commands (`glass`, `workspace`, `doctor`, `inspect`, `diff`, `replay`, `memory`, `backend`) across CLI/MCP/TUI using shared typed results.
8. Add cross-pillar integrated fixtures, security/privacy checks, performance metrics, and migration documentation.

## Integration inventory

The final plan must verify these real call chains, not isolated mocks:

```text
BrowserSession → BrowserBackend → capability negotiation → compiler/runtime policy
BrowserSession → BrowserPresentation → TerminalGraphicsBackend → Ratatui workspace
BrowserSession → fresh multi-surface extraction → Web IR → memory validation → compiler
WorkspaceCoordinator → profile/session/memory/TUI/MCP/daemon attachments
Human input → mutation lease → BrowserSession → fresh extraction → workflow reconciliation
Task compiler → provenance envelope (live/memory/surface/backend/portability)
CLI/TUI/MCP/daemon/Rust API → shared resource references, errors, revisions, and results
```

## Acceptance gates for this local sprint

- Every foundation contract has bounded types, typed invalid-input errors, and focused tests.
- No new foundation module duplicates an existing knowledge/session/TUI contract.
- Each foundation task is committed with a conventional commit before review.
- Review agents inspect committed code against this analysis and issue #31; blocking findings require fixes before merge.
- Integration is merged locally only. No `git push`, tag, `cargo publish`, or GitHub release operation is permitted.
- The final local report distinguishes completed implementation and validation
  from the separate tag, push, registry publication, and GitHub Release steps.

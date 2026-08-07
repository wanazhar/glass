# Experience Layer

Status: Accepted for issue #31 gates

Glass exposes one capability-oriented experience contract through Rust, CLI,
MCP, and the local daemon. Frontends do not create a second browser data
plane: they validate the same workspace, backend, surface, policy, memory,
verification, portability, and workflow evidence.

## Evidence rules

* Memory is advisory. A fresh Web IR revision, current policy, and declared
  backend/surface capability remain executable authority.
* Surface understanding is progressive. Opaque and coordinate-only surfaces
  cannot compile semantic actions; executable claims require current evidence
  and provenance.
* Backend profiles explicitly declare support, limitations, dependencies,
  certification, and portability. Omitted capabilities are unavailable and
  fail closed.
* Workspace references carry profile/storage and, for ephemeral workspaces,
  a generation. A reference from another incarnation is rejected.
* Result projections are bounded. Detailed evidence is local and redacted;
  a result ID is returned when details are available.

## References, ownership, and recordings

`resourceRefs` contains typed `glass://workspace/...` references. A reference
is scoped to a workspace profile and, for ephemeral workspaces, its generation;
clients MUST reject a reference whose scope or generation is not current.
`provenance` identifies the producing interface and whether evidence is
authoritative. Provenance is explanatory only and MUST NOT grant a mutation
capability.

Workspace mutation is serialized by one revision-guarded actor lease. Daemon
clients must acquire the lease for mutation tools, retain the current token,
and re-observe after expiry or revision conflict. Observe, resolve, extract,
verify, and replay operations do not acquire a mutation lease.

Replay, diff, and attach consume only versioned, redacted recording bundles.
They validate exact scenario and content hashes, enforce event and serialized
size budgets, and never start Chrome or attach to an external browser.
Attach means attaching validated evidence to an experience result, not taking
control of a browser. Recording values, cookies, screenshots, and secrets are
not accepted by the replay contract.

## Interface map

| Contract | CLI | MCP | Browser-free Rust path |
|---|---|---|---|
| Workspace | `glass workspace list|inspect|suspend|resume|delete` | `workspaceStatus`, `workspaceInspect` | `WorkspaceStore`, `WorkspaceSession` |
| Memory | `glass memory status|inspect|explain|forget|export|prune|reindex` | `memoryStatus`, `memoryInspect`, `memoryExplain`, `memoryForget`, `memoryExport`, `memoryPrune`, `memoryReindex` | `KnowledgeStore` |
| Surfaces | `glass surfaces inspect|coverage` | `surfaceInspect` | `SurfaceSet::from_json` |
| Backend | `glass backend status|capabilities|test` | `backendStatus`, `backendTest` | `BackendProfile`, `BrowserBackendDispatcher` |
| Replay | `glass replay inspect|diff|attach` | `replayInspect` | `ReliabilityReplayBundle` |

The `glass://contract/experience` MCP resource is the discoverability and
safety summary. `doctor --json` remains the environment diagnostic; it does
not claim that a missing browser or partial backend has real-browser parity.

## Real versus partial gates

The ProofBackend/reliability paths are deterministic, browser-free evidence
and are not real browser performance or compatibility claims. CDP is the real
browser transport currently exercised by `BrowserSession`; other backend
profiles are declarations until their adapter and conformance evidence are
present. Partial surface coverage is reported, never upgraded implicitly.

# Ownership and compatibility boundaries

Glass is maintained as a local development environment and browser control
plane. Every public operation has one owning layer; sharing result types across
interfaces does not transfer lifecycle or mutation authority between layers.

| Surface | Owner | Contract |
| --- | --- | --- |
| CDP transport and Chrome lifecycle | `src/browser/cdp.rs`, `chrome.rs` | typed transport errors and bounded deadlines |
| Browser state and guarded actions | `src/browser/session/` | revision-scoped references and policy gates |
| Agent operations | `src/browser/session/agent.rs` | inspect, target, act-and-verify, extraction, recovery |
| Extraction engine | `src/extraction.rs`, `src/browser/session/observe.rs` | strict request scopes, live source acquisition, explicit omissions, and resource budgets |
| Glass Web IR v1 | `src/web_ir.rs` | deterministic evidence reconciliation, bounded entity metadata, and graph invariants |
| Workflow contract and execution | `src/browser/session/workflow.rs` | validated steps, bounded execution, output proofs |
| Workflow recording | `src/browser/session/workflow/recorder.rs` | redacted semantic drafts and review-required evidence |
| Workflow checkpoints | `src/browser/session/workflow/checkpoint.rs` | bounded resume state and reconciliation |
| Redacted state artifacts | `src/browser/session/snapshot.rs` | versioned, bounded, secret-safe snapshots |
| MCP envelope and tool registry | `src/mcp/` | JSON-RPC framing and stable tool schemas |
| CLI dispatch | `src/cli/` | deterministic command parsing and output projections |
| Protocol compatibility | `src/protocol.rs`, `src/capabilities.rs` | additive fields and negotiated schemas |
| Development Runtime | `src/development/` | canonical project roots, bounded files, PTYs, events, actors, graphs, replay, and experiments |
| TUI presentation | `src/tui/` | responsive views and input routing; no independent mutation semantics |
| Local daemon | `src/daemon/` | resident session ownership, local transport, mutation leases, and shutdown |
| Repository clients | `clients/typescript/`, `clients/python/` | typed convenience projections over negotiated MCP; no server authority |

## Change rules

1. Preserve the canonical Rust result type when adding CLI and MCP operations.
2. Additive serialized fields require defaults or explicit negotiation.
3. A mutation must retain policy checks, revision checks, and transaction evidence.
4. Failure responses must identify phase, retry classification, and whether an
   external effect remains possible.
5. Snapshot and diagnostic artifacts must redact DOM, screenshots, credentials,
   query strings, evaluated expressions, and raw page text.
6. New client examples must start the local `glass` binary; Glass does not
   publish browser runtimes or separate client packages.

## Lifecycle rules

- A one-shot CLI invocation owns only the session it starts. Do not compose
  stateful examples from separate invocations unless a daemon or manifest owns
  the shared session.
- An owned browser receives `Browser.close` before fallback termination. Attach
  mode observes an external lifecycle and never closes that browser.
- A resident project belongs to the TUI, MCP server, or daemon registry that
  created it. Client disconnect does not silently terminate a daemon-owned PTY.
- TUI and client helpers may render or project canonical results, but policy,
  revision, lease, and actor checks remain in the owning runtime.
- Repository clients must negotiate schemas and capabilities with the exact
  matching source-line executable; presence of a helper method is not evidence
  that a connected server supports it.

## Failure ownership

The layer that detects a failure assigns its stable kind and phase. Adapters may
add transport context but must not convert ambiguity, stale revision, denied
policy, expired cursor, or lost mutation lease into a transparent retry. A
caller may retry only after the canonical result says whether an external
effect remains possible and after it refreshes the relevant state.

Focused tests live beside the owning module. Cross-interface compatibility is
validated by `tests/protocol_conformance.rs` and the checked-in fixtures.

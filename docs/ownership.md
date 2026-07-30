# Ownership and compatibility boundaries

Glass is maintained as a local browser control plane. Every public operation has
one owning layer:

| Surface | Owner | Contract |
| --- | --- | --- |
| CDP transport and Chrome lifecycle | `src/browser/cdp.rs`, `chrome.rs` | typed transport errors and bounded deadlines |
| Browser state and guarded actions | `src/browser/session/` | revision-scoped references and policy gates |
| Agent operations | `src/browser/session/agent.rs` | inspect, target, act-and-verify, extraction, recovery |
| Workflow contract and execution | `src/browser/session/workflow.rs` | validated steps, bounded execution, output proofs |
| Workflow recording | `src/browser/session/workflow/recorder.rs` | redacted semantic drafts and review-required evidence |
| Workflow checkpoints | `src/browser/session/workflow/checkpoint.rs` | bounded resume state and reconciliation |
| Redacted state artifacts | `src/browser/session/snapshot.rs` | versioned, bounded, secret-safe snapshots |
| MCP envelope and tool registry | `src/mcp/` | JSON-RPC framing and stable tool schemas |
| CLI dispatch | `src/cli/` | deterministic command parsing and output projections |
| Protocol compatibility | `src/protocol.rs`, `src/capabilities.rs` | additive fields and negotiated schemas |

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

Focused tests live beside the owning module. Cross-interface compatibility is
validated by `tests/protocol_conformance.rs` and the checked-in fixtures.

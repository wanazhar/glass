---
id: native-engine-browser-144
scope: glass-browser/native-engine/target-lifecycle
status: completed
depends-on: [native-engine-browser-143]
---

# Native engine browser slice 144: target lifecycle ownership

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Replace the native backend's single-engine topology with a bounded registry of
independently owned page targets. Target creation, explicit selection, and
closure must remain inside `glass-browser` and use the same native engine
owner as navigation and browser evidence.

## Contract

- the connected native session starts with the caller's context ID as its
  active target and preserves that stable identity;
- new targets receive collision-free `native-target-N` IDs, inherit the
  session's native configuration, initialize independently, and record the
  active target as their opener;
- every target owns its native document, history, content worker, request
  ledger, prompt queue, download queue, and target-local browser state;
- target projections expose stable IDs, redacted URL/title snapshots, opener
  linkage, and exactly one explicitly selected active target;
- selecting a parked target swaps complete native engine ownership without
  navigating or resetting the target that was left behind;
- closing either the selected or a parked target closes its native resources;
  closing the session drains all remaining target engines and is idempotent;
- the registry enforces the existing topology limit of 32 targets and fails
  closed for unknown IDs, invalid URLs, closed engines, and a session with no
  selected target;
- runtime, CLI, and MCP target create/select/list/close calls reach the native
  owner without creating Chromium or falling through to CDP;
- target lifecycle tests demonstrate independent navigation state, opener
  identity, selection, parked-target retention, and complete cleanup;
- child browsing contexts, popup-default-action creation, and frame-scoped
  operations remain separate follow-on ownership work and are not implied by
  page-target support.

## Tradeoffs

The backend keeps one explicit active engine lock and parks complete engine
instances behind stable target IDs. This keeps existing browser operations
revision-safe and avoids duplicating every operation with a second implicit
target parameter, at the cost of serializing target selection with active
target operations. A target is initialized before publication, so callers
never observe a half-created page; failed initialization is closed and
removed.

The public CLI is one-shot, so persistent target workflows are primarily
useful through a long-lived runtime session or MCP server. The target limit
and redacted projections bound memory and avoid exposing profile or page
secrets through discovery. Target-local queues are preserved with their
engine, while profile persistence continues through the existing profile
ownership layer.

## Implementation surface

- `browser/native_backend.rs`: target registry, stable IDs, opener linkage,
  explicit selection, bounded creation/closure, and all-target shutdown;
- `browser/native_engine/engine.rs`: deterministic drop cleanup for parked or
  failed target engines and storage-reader leases;
- `browser/runtime.rs`: native runtime target lifecycle routes;
- `cli/runner.rs`, `mcp/server.rs`: native create/close dispatch;
- `tests/native_engine.rs`, `mcp/server.rs`: lifecycle and state-retention
  regressions without Chromium;
- synchronized architecture, analysis, plan, and package README records.

## Verification

```text
cargo fmt --all -- --check
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_session_preserves_independent_target_state_and_lifecycle -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --lib native_mcp_routes_target_lifecycle_without_chromium -- --nocapture
```

The target-lifecycle integration and dedicated MCP lifecycle tests pass with
independent first/second/third target navigation, selection, closure, and
session cleanup. The feature-gated test-target check passes. Full native
integration and the package-wide release gates remain required after the next
implementation batches.


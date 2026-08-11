# runtime-035-002 — native Pi AgentSession SDK boundary

Status: Complete locally on 2026-08-11. No push or publication performed.

## Outcome

Resident agents now use the Glass-owned `GlassPiRuntime`, which launches Node
directly on an embedded framed runtime and imports Pi's `AgentSession` SDK.
The active `glass-dev` path no longer imports `PiHarness`, uses the legacy
browser `HarnessRequest`, or invokes the Pi CLI RPC mode.

Pi receives exactly one executable `glass_tool`; SDK built-ins and repository
resources are disabled. Tool calls are routed directly through the durable
workspace actor with its existing trust, authorization, generation, revision,
and actor context. Session operations have a Pi-specific contract instead of a
generic future-agent adapter.

## Evidence

- `crates/glass-dev/src/pi_runtime.rs` owns child lifecycle, SDK discovery,
  cache materialization, bounded framing, direct requests, event flow, tool
  brokerage, session confinement, and explicit errors.
- `crates/glass-dev/assets/pi-runtime.mjs` owns SDK session/resource setup and
  every direct session operation required by the issue.
- `packages/pi-runtime` pins the native SDK at `0.84.1` without installing a
  duplicate dependency tree in this checkout.
- The real installed SDK test creates an `AgentSession` and exercises hello,
  metadata, statistics, tree/messages, create/list, and incompatible selection.
- Agent registry and routed agent tools use `PiSessionRequest`; no
  `HarnessRequest` or `PiHarness` remains under `crates/glass-dev`.

## Validation

```text
node --check crates/glass-dev/assets/pi-runtime.mjs
npm --prefix packages/pi-runtime run check
cargo test -p glass-dev pi_runtime::tests -- --nocapture
```

All passed. Full `glass-dev` tests and strict Clippy are the checkpoint commit
gate. Removing the now-dead browser-owned legacy implementation is tracked by
the following product-boundary checkpoint rather than hidden in this runtime
change.

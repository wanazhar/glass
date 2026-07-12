---
id: wait-010
scope: explicit browser waits
status: done
depends-on: [mcp-008, target-009]
---

# Add typed event-driven waits

## Objective

Provide cancellable waits with explicit conditions and deadlines so callers do
not rely on sleeps or a universal page-ready heuristic.

## Context

- `docs/architecture/automation.md`
- `docs/architecture/browser.md`

## Path

- `src/browser/cdp.rs`
- `src/browser/session.rs`
- `src/cli/`
- `src/mcp/`
- `tests/`
- `docs/cli.md`
- `docs/mcp.md`

## Requirements

- Support lifecycle, URL, target state, text, JavaScript predicate, and bounded
  network-quiet conditions.
- Prefer events; use bounded polling only when CDP has no reliable event.
- Make deadlines and cancellation explicit.
- Return the last bounded state and diagnostic reason on timeout.
- Refactor navigation to use the same wait machinery.

## Verification

- Deterministic delayed-content, SPA, redirect, never-idle, cancellation, and
  timeout tests.
- Real MCP cancellation reaches an active wait and leaves the session usable.
- No hidden sleeps enter fast actions.

## Completion

- Added typed lifecycle, exact/prefix URL, target attached/visible/hidden/
  enabled/stable, visible-text, boolean JavaScript, and bounded network-quiet
  conditions with mandatory positive deadlines.
- Waits subscribe to existing Page/DOM events and use a 50 ms bounded polling
  fallback for semantic state. Navigation now uses the same typed lifecycle
  timeout surface while subscribing before `Page.navigate` so fast loads and
  redirects cannot be missed.
- Network quiet enables Network only for the wait, bounds retained request IDs
  to 1,024 plus an overflow counter, and disables the domain synchronously on
  success or through a drop guard on timeout/cancellation.
- Timeout errors expose only a typed condition, deadline, 512-byte last state,
  and reason. MCP serializes this safe structure; dropping an MCP wait cancels
  it and the serialized browser session remains usable.
- CLI `wait CONDITION --timeout-ms` and MCP `wait` share the same parser and
  default 10-second deadline. Fast click/action paths were not changed.
- Deterministic real-Chrome coverage includes delayed content, SPA URL change,
  HTTP redirect, target stability, never-idle network traffic, timeout state,
  local cancellation recovery, and MCP wait cancellation recovery.
- On Linux/aarch64, a 50-iteration warm `js=true` wait measured 0.367 ms p50
  and 0.478 ms p95. The stripped release binary is 4,595,704 bytes (+65,568
  from `target-009`), compact context is 15,835 bytes, and post-workload Glass
  RSS is 6,922,240 bytes. All remain within release gates; workflow RSS is the
  documented allocation proxy.

## Commit

`feat: add typed cancellable browser waits`

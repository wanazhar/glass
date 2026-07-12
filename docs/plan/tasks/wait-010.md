---
id: wait-010
scope: explicit browser waits
status: in-progress
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

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
  enabled/stable, untruncated visible-text substring, boolean JavaScript, and
  bounded network-quiet conditions with deadlines limited to 1-300,000 ms and
  condition strings limited to 4 KiB.
- Waits subscribe to existing Page/DOM events and use a 50 ms bounded polling
  fallback for semantic state. Navigation now uses the same typed lifecycle
  timeout surface while subscribing before `Page.navigate` so fast loads and
  redirects cannot be missed.
- Network quiet uses a ref-counted Network-domain lease across overlapping
  waits. A serialized async state machine holds the lease lock across Network
  enable/disable acknowledgements so acquire, cancellation, and final release
  cannot reorder domain transitions. It bounds retained request IDs to 1,024, fails conservatively on event
  lag or overflow, and disables the domain after the final success, timeout,
  error, or cancellation lease is released. Its observation window explicitly
  begins when the first lease enables Network.
- Timeout errors expose only a typed condition, deadline, 512-byte last state,
  and reason. MCP serializes this safe structure; dropping an MCP wait cancels
  it and the serialized browser session remains usable.
- CLI `wait CONDITION --timeout-ms` and MCP `wait` share the same parser and
  default 10-second deadline. Fast click/action paths were not changed.
- Every condition check, including a never-resolving JavaScript promise and
  the navigation command/page-info calls, is wrapped by the single overall
  deadline. CLI and MCP navigation accept the same bounded timeout.
- Visible-text polling traverses rendered text nodes and rejects opacity-zero,
  hidden, zero-area, and fully clipped ancestors rather than trusting
  `body.innerText`.
- Deterministic real-Chrome coverage includes every condition variant, delayed
  content, SPA URL change, HTTP redirect, target stability, overlapping
  network leases, never-idle traffic, timeout state, pending-CDP deadline,
  local cancellation recovery, and MCP wait cancellation recovery.
- On Linux/aarch64, a 50-iteration warm `js=true` wait measured 0.367 ms p50
  and 0.478 ms p95. The stripped release binary is 4,595,704 bytes (+65,568
  from `target-009`), compact context is 15,835 bytes, and post-workload Glass
  RSS is 6,922,240 bytes. All remain within release gates; workflow RSS is the
  documented allocation proxy.

## Commit

`feat: add typed cancellable browser waits`

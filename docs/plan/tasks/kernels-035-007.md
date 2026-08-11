# kernels-035-007 — governed persistent-kernel capabilities

Status: Complete locally on 2026-08-11. No remote mutation performed.

Python, JavaScript, shell, and SQL kernels now expose bounded Glass tool calls.
Every call re-enters the shared DevelopmentToolRouter with a session allowlist,
current revision checks, trust and mutation policy, stable `kernel:<name>`
executor identity, original initiator provenance, graph links, and replay
events. Nested `glass.eval.*` calls are forbidden and each execution is capped
at 32 tool calls.

Cancellation terminates the owned backend and marks it failed; reset recreates
the backend without changing capability policy. Timeout remains a hard
termination path. Router, MCP inventory, and TUI expose start, execute, list,
cancel, reset, and stop.

Real language tests cover Python/JavaScript calls and shell/SQL calls. A
workspace integration test proves the nested read uses the same router and
records initiator plus executor, while a granted write capability without
mutation authority is still denied.

The public guide explicitly states that Python, JavaScript `vm`, shell, and SQL
bindings are not OS sandboxes.

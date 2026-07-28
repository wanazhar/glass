# Stable runtime platform foundation

Status: in progress locally; not published until 0.2.0.

This phase covers the first platform boundary for issue #27:

- capability and schema negotiation is exposed by MCP initialization and
  `glass capabilities`;
- Rust, TypeScript, and Python clients request and retain the manifest;
- action, session-checkpoint, workflow-checkpoint, and workflow-trace schemas
  are published;
- local Unix daemon lifecycle is available through `glass daemon start`,
  `status`, `doctor`, and `stop`;
- daemon sessions are local-only, isolated MCP child processes, bounded to
  four concurrent clients, and protected by a mode-0600 socket;
- `glass doctor` reports browser, daemon, profile, policy, store, and
  extension-loader state;
- extension manifests validate exact host/action permissions, but extension
  code is not loaded.

The remaining platform work includes a shared daemon session protocol with
explicit ownership and mutation leases, restart/recovery semantics, a real
extension host, cross-transport golden scenarios, and the complete client/TUI
contract inventory. This task must not be marked complete until those paths
have executable conformance coverage.

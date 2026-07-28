# Stable runtime platform foundation

Status: in progress locally; not published until 0.2.0.

This phase covers the first platform boundary for issue #27:

- capability and schema negotiation is exposed by MCP initialization and
  `glass capabilities`;
- Rust, TypeScript, and Python clients request and retain the manifest;
- action, session-checkpoint, workflow-checkpoint, and workflow-trace schemas
  are published;
- local Unix daemon lifecycle is available through `glass daemon start`,
  `status`, `doctor`, `logs`, and `stop`;
- daemon sessions are local-only, share one runtime session namespace, are
  bounded to four concurrent clients, and are protected by a mode-0600 socket
  plus same-user peer credentials on Linux;
- daemon clients negotiate `localDaemon`, use owner-bound acquire/renew/release
  lease methods, and mutation calls fail closed without a valid lease token;
- daemon shutdown handles SIGTERM, releases leases after in-flight requests,
  closes the shared browser session, and removes its local socket artifacts;
- daemon startup recovery and stale-socket cleanup have executable Unix
  integration coverage;
- Python and TypeScript clients can connect to the daemon socket and expose
  the same mutation-lease lifecycle helpers.
- `glass doctor` reports browser, daemon, profile, policy, store, and
  extension-loader state;
- extension manifests validate exact host/action permissions, but extension
  code is not loaded.

The remaining platform work includes active-run classification beyond stale
socket recovery, a real extension host, broader cross-transport golden
scenarios, and the complete client/TUI contract inventory. This task must not
be marked complete until those paths have executable conformance coverage.

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
- policy failures expose a versioned rule/phase/remediation contract;
- the TUI displays the negotiated schema count and daemon capability state;
- daemon operations share a bounded global in-flight request budget.
- daemon status records bounded owner-scoped active workflow requests, and
  shutdown/stale startup recovery logs each interrupted request as requiring
  checkpoint reconciliation.
- interrupted workflow records persist as a versioned recovery artifact and
  are surfaced by `glass daemon doctor`.
- recovery state can be cleared only through an explicit acknowledgement
  command; acknowledgement does not resume work or grant a lease.
- daemon request governance enforces both a global in-flight limit and a
  per-client limit.
- the TUI exposes read-only daemon status, doctor, logs, and recovery views
  without starting a browser operation.
- Python and TypeScript SDKs expose bounded capability and schema support
  checks before optional operation dispatch.
- the transport-neutral protocol envelope is published as schema v1 and MCP
  tool calls validate against the same canonical operation mapping.
- two first-party reference extensions exercise the manifest, permission, and
  bounded host protocol without enabling the unfinished extension capability.
- the extension host exposes an explicit Linux/macOS native-sandbox path that
  fails closed when its platform boundary is unavailable.
- `glass doctor` reports browser, daemon, profile, policy, store, and
  extension-loader state;
- extension manifests validate exact host/action permissions, but extension
  code is not loaded.

The remaining platform work includes guarded executor integration for
extensions, checkpoint-acknowledgement handling for active runs, broader
cross-transport golden scenarios, and the complete client/TUI operation
inventory. This task must not be marked complete until those paths have
executable conformance coverage.

# Local daemon

Glass can supervise local MCP client sessions through a Unix-domain socket on
Linux and macOS. The daemon does not bind a TCP port and does not expose a
remote service. Connected clients use the daemon's shared session namespace;
mutation operations are serialized by an owner-bound lease.

## Lifecycle

```console
glass daemon start
glass daemon status
glass daemon doctor
glass daemon logs
glass daemon acknowledge-recovery --request-id REQUEST_ID [...]
glass daemon stop
```

Use `--socket PATH` and `--status PATH` to select explicit locations. The
default paths are under the platform local-data directory. The socket is
created with mode `0600`. On Linux, the daemon also checks `SO_PEERCRED` and
accepts only clients running as the same operating-system user. On macOS,
socket ownership and mode are the local authentication boundary. The status JSON follows
[glass-daemon-v1.schema.json](schema/glass-daemon-v1.schema.json) and includes
the daemon PID, protocol version, transport, and active client-session count.
The transport identifier is `unix-mcp-shared-session`.
`glass daemon logs` returns at most the last 64 KiB of the local log file.
Status also exposes the current mutation lease owner when one exists; the
lease token is never written to status or logs.
While a workflow request is running, the status also records its bounded
request ID and owner. On shutdown or restart after a crash, those entries are
reported as interrupted and must be reconciled from a checkpoint before a
caller resumes work.
The status points to a versioned recovery record when one exists; `glass
daemon doctor` reports it as `reconciliation_required`.
After reconciling each listed run from a checkpoint, an operator must explicitly
name every recovered request ID with `glass daemon acknowledge-recovery
--request-id REQUEST_ID`. Glass rejects partial, unknown, or duplicate
acknowledgements and only then clears the recovery marker. This never resumes a
workflow or grants a mutation lease.

The daemon is intentionally limited in this release: it provides local
lifecycle supervision and a shared browser session namespace. Clients must
negotiate the daemon capability before using it, then use these MCP methods:

```text
glass/lease/acquire  {"ttlMs": 1000..900000}
glass/lease/renew    {"token": "...", "ttlMs": 1000..900000}
glass/lease/release  {"token": "..."}
```

The returned token is owner-bound to the local socket connection. At most one
client can hold the mutation lease for `daemon-default`; observation tools may
run without it. Mutation `tools/call` requests must carry the token as
`arguments.leaseToken`. A disconnected client releases its lease after its
in-flight requests finish. Each client has at most four in-flight requests,
within a daemon-wide limit of sixteen, so one client cannot consume the entire
request budget. Remote network access is not supported.

# Local daemon

The Glass daemon accepts local MCP clients through a Unix-domain socket. It
runs on Linux and macOS. It does not open a TCP port. It does not provide a
remote service.

The daemon gives each connected client an independent browser-session
namespace. Clients can use the same Unix socket without sharing browser state.
The daemon still shares the process-wide request budget and lease authority;
each client lease is bound to that client's session namespace and owner.

## Start and stop

Run:

```console
glass daemon start
glass daemon status
glass daemon doctor
glass daemon logs
glass daemon stop
```

Use `--socket PATH` and `--status PATH` to set explicit paths. The default
paths are in the platform local-data directory.

## Access control

The socket has mode `0600`.

On Linux, Glass checks `SO_PEERCRED`. Only the same operating-system user may
connect.

On macOS, socket ownership and mode provide the local access boundary.

The daemon does not support remote network access.

## Reuse one live MCP session

Keep one MCP process and transport connection open for a sequence of related
operations. The MCP bridge starts its `BrowserSession` lazily on the first
browser tool call and retains it in the connection's session store; later
calls on that connection reuse the same owned browser process and active target.
The stdio server closes that owned session when its input reaches EOF.

Reconnects are intentionally not a browser-session handoff: a new stdio
process or daemon socket connection gets a new session namespace and must
observe again before acting. This boundary prevents one client from adopting
another client's endpoint or browser state. The daemon is a local MCP
transport and lifecycle owner, not a general-purpose Chrome WebSocket broker.

## Status and recovery

The status document follows
[glass-daemon-v1.schema.json](schema/glass-daemon-v1.schema.json). It includes:

- the daemon process ID;
- the protocol version;
- the Unix transport;
- the active client-session count;
- the current mutation lease owner;
- active workflow request IDs; and
- the recovery state.

The daemon never writes a lease token to status or logs.

`glass daemon logs` returns at most 64 KiB from the local log file.

When the daemon stops or restarts, active workflow requests become interrupted.
A caller must reconcile each request from its checkpoint before it resumes.

The daemon stores recovery data in a versioned record. The doctor command
reports this state as `reconciliation_required`.

After reconciliation, acknowledge every request:

```console
glass daemon acknowledge-recovery --request-id REQUEST_ID [...]
```

Glass rejects partial, unknown, and duplicate acknowledgements. Acknowledgement
does not resume a workflow. It does not grant a lease.

## Mutation leases

Clients must negotiate the `localDaemon` capability before they use daemon
operations.

Use these MCP methods:

```text
glass/lease/acquire  {"ttlMs": 1000..900000}
glass/lease/renew    {"token": "...", "ttlMs": 1000..900000}
glass/lease/release  {"token": "..."}
```

One client may hold the mutation lease for `daemon-default`. Observation
operations do not require the lease. Mutation requests must send the token as
`arguments.leaseToken`.

A disconnected client releases its lease after its active requests finish.
Each client has a maximum of four in-flight requests. The daemon has a maximum
of sixteen in-flight requests.

The lease owner is tied to the local socket connection. Do not write the lease
token to a log or file.

# Local daemon

The Glass development daemon accepts local clients through a Unix-domain
socket on Linux/macOS and a byte-mode named pipe on Windows. It does not open a
TCP port or provide a remote service. Platform certification is recorded only
after the corresponding native CI job passes.

The daemon registry contains at most eight bounded workspace actor handles.
Each actor owns one `DevelopmentWorkspace`, its resident services, and a
64-command queue. Registry borrows cover only lookup/open/list/close; a long
operation in one workspace cannot hold a global workspace-state lock. A 50 ms
actor tick advances autonomous task DAGs without client polling.

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

On Windows, Glass derives a stable per-user pipe name from LocalAppData,
requires the `\\.\pipe\glass-dev-*` namespace, rejects remote pipe clients,
claims the first pipe instance, and still requires the private 256-bit daemon
token. Named-pipe endpoints vanish when their owner exits, while stale status
is rejected using the recorded PID.

The daemon does not support remote network access.

Windows CI natively proves start/status/stop, workspace open, a confined tool
call, fresh-client reconnect, endpoint rejection, and cleanup. Cross-compiling
on Unix is useful source evidence but is never reported as native proof.

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

## Durable workspace actors

Development clients use the daemon process's bounded canonical-root registry.
A retained actor owns buffers, PTYs, language servers, Pi sessions, task DAGs,
kernels, debuggers, experiments, and its authoritative browser service. Fresh
socket clients inspect the same actor identity and resources.

The native daemon protocol provides `workspace.open`, `workspace.list`,
`workspace.inspect`, `workspace.tool`, and `workspace.close`. Close explicitly
shuts the actor down and reaps owned resources; it does not delete project
files, Pi session JSONL, or persistent timeline data.

## Failure and shutdown matrix

| State | Meaning | Operator action |
|---|---|---|
| stale status, dead PID | prior daemon did not cleanly remove metadata | run `daemon doctor`; start performs bounded stale-state recovery |
| socket exists but is not compatible | another local service or malformed endpoint owns the path | do not connect or delete blindly; inspect ownership and use explicit paths |
| lease expired/disconnected | mutation authority is gone | acquire a new lease after observing current state |
| `reconciliation_required` | daemon stopped with active workflow requests | reconcile every request checkpoint, then acknowledge exact IDs |
| project session evicted | resident project resources were closed | reattach the root and restart required processes |

Stop the daemon before uninstalling `glass-dev`. A forced package removal does
not stop an already running daemon process or remove its local data.

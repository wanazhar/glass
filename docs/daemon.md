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
glass daemon stop
```

Use `--socket PATH` and `--status PATH` to select explicit locations. The
default paths are under the platform local-data directory. The socket is
created with mode `0600`. On Linux, the daemon also checks `SO_PEERCRED` and
accepts only clients running as the same operating-system user. On macOS,
socket ownership and mode are the local authentication boundary. The status JSON follows
[glass-daemon-v1.schema.json](schema/glass-daemon-v1.schema.json) and includes
the daemon PID, protocol version, transport, and active client-session count.

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
in-flight requests finish. Remote network access is not supported.

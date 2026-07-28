# Local daemon

Glass can supervise local MCP client sessions through a Unix-domain socket on
Linux and macOS. The daemon does not bind a TCP port and does not expose a
remote service. Each connected client is bridged to an isolated `glass --mcp`
child, so browser sessions are not silently shared across clients.

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
the daemon PID, protocol version, transport, and child-session count.

The daemon is intentionally limited in this release: it provides local
lifecycle supervision and per-client session isolation, but not shared browser
sessions or workflow lease transfer. The reusable lease authority enforces one
owner-bound mutation lease per session with bounded renewal, but the daemon
does not expose shared-session lease messages yet. Remote network access is
not supported. Those features must be negotiated and implemented as separate
protocol additions.

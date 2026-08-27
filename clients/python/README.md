# Glass Python client

The Python client uses only the Python standard library. It starts one local
Glass binary, negotiates MCP, and provides typed helpers for browser actions,
observations, workflows, knowledge, targets, frames, storage, diagnostics,
browser controls, and the complete local Development Runtime.

## Install

From this directory, run:

```console
python3 -m pip install .
```

The client does not install Chrome or Chromium.
It is a repository client for the `0.3.13` source line and is not currently
published to PyPI. Install it from this checkout and pair it with the exact
matching Glass executable.

For server policy, framing, and the complete negotiated inventory, read the
[MCP integration guide](../../docs/mcp.md) and
[tool catalog](../../docs/mcp-tools.md). Browser behavior and safety contracts
remain defined by the server, not this convenience client.

## Start a client

```python
from glass_client import GlassClient

glass = GlassClient(command="/absolute/path/to/glass")
try:
    glass.navigate("https://example.com")
    print(glass.observe_semantic("structured"))
finally:
    glass.close()
```

`close()` stops the client transport and a child server started by this client.
It does not terminate a separately owned daemon, daemon-resident project PTY,
or attached browser. Always close the client from `finally` or a surrounding
application shutdown hook.

Use `daemon_socket="/path/to/glass.sock"` to connect to a running Linux or
macOS daemon.

Call `glass.initialize()` before an optional operation. The call negotiates
Glass schema versions and returns a `GlassCapabilityManifest`. The manifest is
also available as `glass.capabilities`.

Use `supports_capability`, `supports_schema`, and
`require_capability` before you call an optional operation. The client
returns a bounded error when the server does not support the requested item.

`list_tools()` returns the negotiated MCP tool inventory. The repository smoke
test compares this inventory with the versioned client conformance fixture.

## Daemon mutations

A daemon mutation requires a lease. Check the `localDaemon` capability. Then
call `acquire_mutation_lease()`. The client adds the lease token to later
mutation calls.

Release the lease when the mutation sequence ends. Do not store the lease token
in logs or persistent files.

## Frames

The client accepts newline-delimited MCP frames and `Content-Length` frames.
It limits each frame to 4 MiB.

## Errors, cancellation, and compatibility

Calls preserve the server's typed error data. Inspect error kind, phase, retry
classification, and possible-effect state before retrying. Never transparently
replay a mutation after EOF or a timeout. Refresh stale observations, reacquire
expired daemon leases, and handle `cursorExpired` as an explicit event-history
gap.

Watcher `stop` callbacks and bounded wait helpers cancel local polling; they do
not prove that a server mutation already dispatched had no effect.
`initialize()` is the compatibility boundary. Require the negotiated schema or
capability before an optional operation rather than relying only on matching
package version text.

## Development Runtime

Glass 0.3.13 development operations use the negotiated `glass.*` catalog. Use
`call()` with the exact schema returned by `list_tools()`; retired `project.*`
cockpit schemas are not negotiated by the new Glass Dev runtime:

```python
trust = glass.call("glass.workspace.trust.status")
tree = glass.call("glass.file.list")
tasks = glass.call("glass.task.list")
browser = glass.call("glass.browser.state")
print(trust["trust"], len(tree["entries"]), len(tasks), browser["connected"])
```

Executable project services such as tests, LSP, DAP, processes, agents,
kernels, and experiments remain unavailable until the local Glass UI records a
trust decision. External MCP clients can inspect trust but cannot elevate it.
Mutation calls remain confirmation-, revision-, and authority-checked by the
same resident router. Browser observations stay structured-first; screenshots
remain explicit calls.

## Verification and removal

Run `python3 smoke.py` with `GLASS_BINARY` set to the matching built executable;
CI also builds a wheel with `python3 -m pip wheel --no-deps .`. The smoke also
proves untrusted workspaces retain static inspection without permitting
executable project discovery. Uninstall with
the same interpreter used for installation, for example
`python3 -m pip uninstall glass-browser-client`. This removes only the Python package;
it does not remove Glass binaries, profiles, project state, or configuration.
Follow the repository
[uninstall guide](../../docs/installation.md#fully-uninstall-glass) for the
full machine cleanup.

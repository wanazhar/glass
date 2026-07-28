# Glass Python client

The Python client uses only the Python standard library. It starts one local
Glass binary, negotiates MCP, and provides typed helpers for browser actions,
observations, workflows, knowledge, targets, frames, storage, diagnostics,
and browser controls.

## Install

From this directory, run:

`console
python -m pip install .
`

The client does not install Chrome or Chromium.

## Start a client

`python
from glass_client import GlassClient

glass = GlassClient(command="/absolute/path/to/glass")
try:
    glass.navigate("https://example.com")
    print(glass.observe_semantic("structured"))
finally:
    glass.close()
`

Use `daemon_socket="/path/to/glass.sock"` to connect to a running Linux or
macOS daemon.

Call `glass.initialize()` before an optional operation. The call negotiates
Glass schema versions and returns a `GlassCapabilityManifest`. The manifest is
also available as `glass.capabilities`.

Use `supports_capability`, `supports_schema`, and
`require_capability` before you call an optional operation. The client
returns a bounded error when the server does not support the requested item.

## Daemon mutations

A daemon mutation requires a lease. Check the `localDaemon` capability. Then
call `acquire_mutation_lease()`. The client adds the lease token to later
mutation calls.

Release the lease when the mutation sequence ends. Do not store the lease token
in logs or persistent files.

## Frames

The client accepts newline-delimited MCP frames and `Content-Length` frames.
It limits each frame to 4 MiB.

# Glass Python client

The Python client uses only the Python standard library. It starts one local
Glass binary, negotiates MCP, and provides typed helpers for browser actions,
observations, workflows, knowledge, targets, frames, storage, diagnostics,
browser controls, and the complete local Development Runtime.

## Install

From this directory, run:

```console
python -m pip install .
```

The client does not install Chrome or Chromium.

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

## Development Runtime

Project, file, process, diff, replay, graph, breakpoint, experiment, actor,
source-link, and agent-harness operations have typed snake-case helpers:

```python
project = glass.project_inspect("/srv/storefront")
files = glass.project_files(project["root"])
diff = glass.project_diff(project["root"])
glass.agent_prompt("Explain the failing verification", project["root"])
```

`watch_project_events()` yields cursor-bounded pages and accepts a `stop`
callback. `cursorExpired` explicitly reports a gap when the persisted timeline
was compacted:

```python
for page in glass.watch_project_events("/srv/storefront", stop=lambda: done):
    for event in page["events"]:
        print(event["kind"], event["actor"]["id"])
```

Live PNG frames are deliberately separate from this structured event feed.
See [`examples/development_events.py`](examples/development_events.py) for a
complete interruptible watcher.

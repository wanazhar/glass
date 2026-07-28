# @glass-browser/client

The dependency-free TypeScript client starts `glass --mcp`. It provides typed
helpers for navigation, actions, verification, batches, workflows, waits,
semantic observations, knowledge, targets, frames, storage, checkpoints,
diagnostics, and browser controls.

The client does not include Chrome, Chromium, or another browser runtime.

## Build

Run:

`console
npm run build
`

The package exports JavaScript and TypeScript declaration files.

## Start a client

`typescript
import { GlassClient } from "@glass-browser/client";

const glass = new GlassClient({ command: "/absolute/path/to/glass" });
await glass.navigate("https://example.com");
const page = await glass.observeSemantic("structured");
console.log(page);
glass.close();
`

Use `daemonSocket: "/path/to/glass.sock"` to connect to a running Linux or
macOS daemon.

Call `await glass.initialize()` before an optional operation. The call
negotiates Glass schema versions and returns a `GlassCapabilityManifest`. The
manifest is also available as `glass.capabilities`.

Use `supportsCapability`, `supportsSchema`, and
`requireCapability` before you call an optional operation. The client returns
a bounded error when the server does not support the requested item.

`listTools()` returns the negotiated MCP tool inventory. The repository smoke
test compares this inventory with the versioned client conformance fixture.

## Daemon mutations

A daemon mutation requires a lease. Check the `localDaemon` capability. Then
call `acquireMutationLease()`. The client adds the lease token to later
mutation calls.

Release the lease when the mutation sequence ends. Do not store the lease token
in logs or persistent files.

## Frames

The client accepts newline-delimited MCP responses and `Content-Length` frames.
It limits each frame to 4 MiB.

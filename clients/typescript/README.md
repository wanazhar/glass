# @glass-browser/client

The dependency-free TypeScript client starts `glass --mcp`. It provides typed
helpers for navigation, actions, verification, batches, workflows, waits,
semantic observations, knowledge, targets, frames, storage, checkpoints,
diagnostics, browser controls, and the complete local Development Runtime.

The client does not include Chrome, Chromium, or another browser runtime.
It is a repository client for the `0.3.9` source line and is not currently
published to npm. Install/build it from this checkout and pair it with the
exact matching Glass executable.

For server policy, framing, and the complete negotiated inventory, read the
[MCP integration guide](../../docs/mcp.md) and
[tool catalog](../../docs/mcp-tools.md). Browser behavior and safety contracts
remain defined by the server, not this convenience client.

## Build

Run:

```console
npm run build
```

The package exports JavaScript and TypeScript declaration files.

## Start a client

```typescript
import { GlassClient } from "@glass-browser/client";

const glass = new GlassClient({ command: "/absolute/path/to/glass" });
await glass.navigate("https://example.com");
const page = await glass.observeSemantic("structured");
console.log(page);
glass.close();
```

`close()` stops the client transport and a child server started by this client.
It does not terminate a separately owned daemon, daemon-resident project PTY,
or attached browser. Put it in a `finally` block in long-running programs.

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

## Errors, cancellation, and compatibility

Rejected calls preserve the server's typed error data. Inspect the error kind,
phase, retry classification, and possible-effect field before deciding to
retry; never automatically replay a mutation after a transport loss. Refresh
observations after stale revisions, reacquire an expired daemon lease, and
resume event reads from a valid cursor after compaction.

Pass `AbortSignal` to watchers and wait helpers that accept it. Cancellation
stops the local wait; it is not proof that a previously dispatched server-side
mutation had no effect. `initialize()` is the compatibility boundary: require
the schema/capability you consume instead of using package version equality as
a substitute for negotiation.

## Development Runtime

Glass 0.3.9 development operations use the negotiated `glass.*` catalog. Use
the generic typed `call<T>()` boundary with the schema returned by
`listTools()`; retired `project.*` cockpit schemas are not negotiated by the
new Glass Dev runtime:

```typescript
const trust = await glass.call<{ trust: string }>("glass.workspace.trust.status");
const tree = await glass.call<{ entries: unknown[] }>("glass.file.list");
const tasks = await glass.call<unknown[]>("glass.task.list");
const browser = await glass.call<{ connected: boolean }>("glass.browser.state");
console.log(trust.trust, tree.entries.length, tasks.length, browser.connected);
```

Executable project services such as tests, LSP, DAP, processes, agents,
kernels, and experiments remain unavailable until the local Glass UI records a
trust decision. External MCP clients can inspect trust but cannot elevate it.
Mutation calls remain confirmation-, revision-, and authority-checked by the
same resident router. Browser observations stay structured-first; screenshots
remain explicit calls.

## Verification and removal

Run `npm run typecheck`, `npm run build`, and then `node smoke.mjs` with
`GLASS_BINARY` set to the matching built executable. The smoke suite compares
the client's negotiated tool inventory with the checked-in fixture and proves
untrusted workspaces retain static inspection without permitting executable
project discovery.
Remove the local package using the package manager or link method used to
install it. Removing the client does not remove the Glass executable, browser
profiles, project state, or configuration; follow the repository
[uninstall guide](../../docs/installation.md#fully-uninstall-glass) for those
resources.

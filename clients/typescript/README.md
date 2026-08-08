# @glass-browser/client

The dependency-free TypeScript client starts `glass --mcp`. It provides typed
helpers for navigation, actions, verification, batches, workflows, waits,
semantic observations, knowledge, targets, frames, storage, checkpoints,
diagnostics, browser controls, and the complete local Development Runtime.

The client does not include Chrome, Chromium, or another browser runtime.
It is a repository client for the `0.3.2` source line and is not currently
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

## Development Runtime

Typed helpers cover project detection, files, search, editing, diagnostics,
processes, code/runtime/semantic diffs, replay, the Development Graph,
breakpoints, experiments, external actors, source/runtime links, and the local
agent harness:

```typescript
const project = await glass.projectInspect("/srv/storefront");
const files = await glass.projectFiles(project.root);
const diff = await glass.projectDiff(project.root);
await glass.agentPrompt("Explain the failing verification", project.root);
```

`watchProjectEvents()` is an async generator over cursor-bounded event pages.
It retains no unbounded history and reports `cursorExpired` when timeline
compaction created a gap:

```typescript
const controller = new AbortController();
for await (const page of glass.watchProjectEvents("/srv/storefront", {
  signal: controller.signal,
})) {
  for (const event of page.events) console.log(event.kind, event.actor.id);
}
```

Live PNG frames are deliberately not part of this feed. Use structured events
for orchestration and a dedicated latest-frame side channel for visual clients.
See [`examples/development-events.ts`](examples/development-events.ts) for a
complete interruptible watcher.

Project state is resident for the MCP server lifetime, so
`projectRun("dev", "npm run dev", root, false)` can start a persistent PTY and
later calls can inspect, read, or stop it. The cockpit helpers expose session
status/detach, reconnect capsules, attention items, and verification cards:

```typescript
const process = await glass.runUntilHealthy("dev", "npm run dev", {
  root: project.root,
  signal: controller.signal,
});
const event = await glass.waitForEvent(e => e.kind === "testCompleted", project.root);
const card = await glass.projectVerificationCard("Checkout fix", project.root);
await glass.projectCapsuleSave(project.root, { eventCursor: event.id, mobileView: "diff" });
```

`withMutationLease()` releases only a lease it acquired itself.
`onAttentionRequired()` deduplicates needs-attention IDs and stops through an
`AbortSignal`. See
[`examples/remote-cockpit.ts`](examples/remote-cockpit.ts).

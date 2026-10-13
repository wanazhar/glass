# TypeScript extension SDK

A typed client for glass-core's existing subprocess protocol. Runtime dependencies
are limited to Node's standard library. GC does not embed Node or a JavaScript engine.

## Build and run the starter

From the glass-core checkout:

```sh
npm ci --ignore-scripts
npm run build
```

Enable the compiled starter in `~/.config/glass-core/config.toml`. Use an absolute
path because GC starts extensions in the current workspace:

```toml
extensions = ["node /absolute/path/to/glass-core/extensions/typescript/dist/index.js"]
```

Start a new GC session and open `/ts-demo` to see the native preview and diagnostics.
The model can call `ts_preview` to preview a proposed text replacement. It does not
write files or make a model request.

The SDK package is built locally; it has not been published to npm. Another Node
project can install the built package with:

```sh
npm install /absolute/path/to/glass-core/packages/extension-sdk
```

Node 22 or newer is required. TypeScript and Node type declarations are build-only
dependencies. The starter demonstrates imports whose `.js` suffix resolves to
TypeScript during compilation and JavaScript at runtime.

## Define an extension

```typescript
import {
  runExtension,
  stringArgument,
  type Handler,
} from '@glass-core/extension-sdk';

const greet: Handler = (args, context) => {
  context.signal.throwIfAborted();
  return `Hello, ${stringArgument(args, 'name')}.`;
};

await runExtension({
  name: 'greetings',
  version: '1.0.0',
  tools: [{
    definition: {
      name: 'greet',
      description: 'Greet someone by name.',
      concurrency: 'parallel',
      schema: {
        type: 'object',
        properties: { name: { type: 'string' } },
        required: ['name'],
        additionalProperties: false,
      },
    },
    execute: greet,
  }],
});
```

Validate model arguments in handlers. JSON parsing and an object check happen in
the SDK; a TypeScript annotation does not enforce your JSON Schema at runtime.
`stringArgument` checks a required string. Throw `ToolError(code, message)` for an
actionable failure. Ordinary exceptions also produce an error result, allowing GC
to report the failure without leaving the host waiting for a reply.

Use `serial` for tools that mutate shared state. Use `keyed` only when its host
scheduling behavior matches your tool; the SDK does not create a second scheduler.

## Lazy loading and persistence

Keep the entry module small. Supply `load` instead of `execute` for a large tool:

```typescript
load: () => import('./implementation.js')
```

The imported module exports an `execute: Handler`. It is loaded on the first tool
call and retained for later calls. A failed import can be retried. There is one
persistent process per configured extension, not a new Node process per call.

GC starts that process to discover the manifest. Lazy loading defers the tool
implementation and its dependencies, not the handshake process. Disabled
extensions start no process. The SDK has no per-token hooks or polling timer.
Keep handshake imports and prompt contributions small; tool schemas still consume
model context when declared.

## Host tools and durable state

A handler's context exposes:

| Member | Use |
| --- | --- |
| `workspace`, `model` | Values supplied by GC's handshake |
| `callId`, `lineage` | Originating tool call and host-owned ancestry |
| `signal` | Shutdown and transport cancellation |
| `callTool(name, arguments)` | Request `bash` or `state` under GC's applicable policy |
| `appendCustom(kind, data)` | Queue a namespaced session entry |
| `readCustom(kind)` | Read namespaced session entries |
| `log(message)` | Write a diagnostic to stderr |

State helpers prefix short kinds with the extension name: `notes` becomes
`my-extension/notes`. An already qualified kind in that namespace is preserved.
Seeded entries retain their wire names.

The current host bridge permits `bash` and `state`. It does not recursively call
extension tools or expose every model tool. The SDK types reflect that contract.

Host replies are correlated by call ID. Pending calls have timeouts and a maximum
count of 16. A custom-entry append acknowledges a queued record; GC persists it
when the callback completes. An interrupted callback may lose that queued entry.

Do not write logs to stdout: it belongs to the protocol. Use `context.log` or
`console.error`. Use `signal` for cooperative cleanup. GC currently cancels a
running extension by stopping its process; the SDK cannot promise to reuse a
process after cancellation. It aborts pending work on shutdown, EOF, output-pipe failure or SIGTERM and
calls `dispose` for cleanup when the process can exit normally. SIGKILL cannot run
cleanup. Await nested host calls before returning your tool result.

## Native previews and diagnostics

Return a `ToolResult` instead of a string:

```typescript
return {
  text: 'Proposed change; no file has been written.',
  preview: {
    title: 'Greeting',
    language: 'typescript',
    before: 'const greeting = "Hello";',
    after: 'const greeting = "Welcome";',
  },
  diagnostics: [{
    severity: 'warning',
    file: 'src/greeting.ts',
    line: 1,
    message: 'Check callers before applying this change.',
  }],
};
```

The SDK serializes these fields as Markdown. GC's native output inspector renders
code blocks and diagnostic tables; narrow terminals use its existing wrapping.
Inline tool cards retain bounded previews. Open `/outputs` for full inspection.
Extension slash commands open the native Markdown viewer. The SDK does not run a
compiler or manufacture diagnostics: the handler supplies findings from its own
checks. A preview does not apply a file edit.

The wire protocol remains version 1. A reply is capped at 4 MiB, matching GC's
host cap; return a bounded summary and a file reference for larger results. Host
input and queued callbacks are also bounded. These limits protect transport memory,
not the extension's own allocations.

## Hooks and permissions

Typed hooks cover authorization, result rewriting, turn completion, seeded session
state, and cleanup. Defining `authorize` sets `authorization_required: true`;
exceptions return a denial. Do not use optional authorization as a security gate.

The SDK is not a sandbox. The extension process has the access allowed by GC's
extension configuration and OS isolation. See [Permissions](../../docs/security.md)
and the [extension contract](../../docs/extending.md) before enabling third-party code.

## Development checks

```sh
npm test
```

Tests exercise a real Node subprocess: handshake, lazy loading, persistent state,
nested calls, input errors, fail-closed authorization, output bounds and shutdown.
EOF and SIGTERM cannot undo host tool effects that have already completed.

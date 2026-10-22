# @glass-core/runtime-client

A TypeScript client for the optional GC host runtime. It starts a separate
`gc runtime` process and does not depend on GC's terminal UI or engine internals.
Node 22 or newer is required.

Build from this checkout:

```sh
npm ci
npm run build
```

The package is not published automatically. Applications can use a reviewed
local package or package archive. Install the GC executable separately.

## Host a conversation

```ts
import {RuntimeClient, type ToolRequest} from '@glass-core/runtime-client';

const gc = new RuntimeClient('gc', {cwd: '/absolute/workspace'});
gc.on('runtimeEvent', event => {
  if (event.type === 'text') process.stdout.write(String(event.data.text));
});
gc.on('toolRequest', async (requestId: string, call: ToolRequest) => {
  try {
    // Apply host permissions and validate arguments before execution.
    const text = await executeApprovedTool(call);
    await gc.toolResult(requestId, {text});
  } catch (error) {
    await gc.toolResult(requestId, {text: String(error), isError: true});
  }
});

await gc.initialize('/absolute/private/sessions');
const {sessionId} = await gc.newSession({
  cwd: '/absolute/workspace',
  toolMode: 'host',
  tools: [{
    name: 'read_file',
    description: 'Read a file through the host.',
    inputSchema: {
      type: 'object',
      properties: {path: {type: 'string'}},
      required: ['path'],
    },
  }],
});
await gc.prompt(sessionId, {
  text: 'Review the supplied project context.',
  context: [{type: 'json', value: {project: 'example'}}],
});
await gc.shutdown();
gc.close();
```

`executeApprovedTool` is application code. A question tool can wait for a custom
answer in the same callback. GC does not execute host tools itself.

## Controls and data

Requests return promises correlated with their response IDs. Use `steer` or
`cancel` while a prompt promise is pending. Listen for `tool_cancel` and stop
the matching host operation, then call `acknowledgeCancel` with its actual
execution status. Unknown effects require host reconciliation before further
mutations.

`progress` sends bounded display updates. `toolResult` accepts text, JSON,
images, and structuredContent. `entries` reads stable journal entries using
byte cursors. `complete` requests a tool-free insertion without adding to the
conversation. Other session and discovery methods remain available through
the generic typed `request<T>` method.

Keep event listeners short. Node pipe backpressure limits unconsumed output;
the SDK bounds pending requests and queued writes to 64 each. It rejects
oversized frames and fails pending requests on disconnect. Protocol errors
are `RuntimeError` values with code and data.kind.

The client emits `diagnostic` for stderr, `runtimeError` for transport failure,
and `closed` when the process closes. Closing a client does not undo tools
executed by the host. Cancel and settle active work before graceful shutdown.

See the [protocol reference](../../docs/reference/runtime.md) for ownership,
limits, persistence, and cancellation semantics.

## Verification

```sh
GLASS_CORE_TEST_BINARY=/absolute/path/to/glass-core npm test
```

The SDK test drives the actual executable without making a provider request.
The repository's external conformance and live Kilo host scripts exercise
model turns and tool execution separately.

# Glass protocol

Glass uses MCP JSON-RPC for transport. It uses a versioned Glass envelope for
operation semantics.

The envelope is defined by
[glass-protocol-v1.schema.json](schema/glass-protocol-v1.schema.json) and the
Rust `glass::protocol` module.

## MCP mapping

| MCP field | Glass field |
|---|---|
| JSON-RPC `id` | `requestId` |
| `params.name` | `operation` with the `browser.` prefix |
| `params.arguments` | `payload` |
| Daemon session and lease context | `sessionId` and `mutationLease` |

Browser-free Web IR inspection and revision tools use canonical operation names
rather than the `browser.` prefix:

| MCP tool | Glass operation |
|---|---|
| `inspectWebIr` | `webIr.inspect` |
| `validateWebIr` | `webIr.validate` |
| `diffWebIr` | `webIr.diff` |
| `continuityWebIr` | `webIr.continuity` |

`inspectWebIr` and `validateWebIr` carry one validated draft graph. Inspection
returns bounded graph metadata; validation returns bounded validity metadata.
Revision tools carry two validated draft graphs and return bounded summaries or
one continuity classification. Entity payloads and page content are not
returned by the MCP projection.

Browser-free Task Protocol tools also use canonical operation names:

| MCP tool | Glass operation |
|---|---|
| `compileTask` | `task.compile` |
| `validateTask` | `task.validate` |

The checked-in [protocol golden fixture](../tests/fixtures/protocol-golden-v1.json)
covers read, leased mutation, workflow, browser-free Web IR inspection and
revision operations, Task Protocol validation and compilation, success, and
typed-error envelopes. Rust tests round-trip the fixture. MCP tests check the
canonical mapping.

The CLI and SDKs use the same operation names and payload fields. MCP keeps
JSON-RPC success and error framing. The operation payload remains bounded and
versioned.

A request must contain protocol version `1`, a non-empty operation name, and a
bounded request ID. A response contains one result or one structured error.
The checked-in [protocol golden fixture](../tests/fixtures/protocol-golden-v1.json)
covers read, leased mutation, workflow, browser-free Web IR inspection and
revision operations, success, and typed-error envelopes. Rust tests round-trip
the fixture. MCP tests check the canonical mapping.

## Compatibility

Glass rejects unknown envelope fields. Additive fields must be optional. A
change to a required field or validation meaning requires a new schema version
and a migration note.

Protocol deadlines are limited to fifteen minutes. Each transport keeps its
own frame limits.

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

`inspectWebIr` and `validateWebIr` carry one validated Glass Web IR v1
document. Inspection returns bounded graph metadata; validation returns bounded
validity metadata. Revision tools carry two validated Web IR documents and
return bounded summaries or one continuity classification. Entity payloads and
page content are not returned by the MCP projection.

MCP dispatch constructs these canonical requests and invokes typed protocol
helpers; MCP-only response options are excluded from canonical payloads.

Browser-free Task Protocol tools also use canonical operation names:

| MCP tool | Glass operation | Canonical payload |
|---|---|---|
| `compileTask` | `task.compile` | `task` plus the source `ir` |
| `validateTask` | `task.validate` | `task` |

The checked-in [protocol golden fixture](../crates/glass-browser/tests/fixtures/protocol-golden-v1.json)
covers read, leased mutation, workflow, browser-free Web IR inspection and
revision operations, Task Protocol validation and compilation, success, and
typed-error envelopes. Rust tests round-trip the fixture. MCP tests check the
canonical mapping.

The fixture includes bounded `taskValidation`, `taskCompilation`, and
`webIrValidation` details for validation, compilation, diff, and continuity
preflight error envelopes, including machine-readable paths and reasons.

The CLI and SDKs use the same operation names and payload fields. MCP keeps
JSON-RPC success and error framing. The operation payload remains bounded and
versioned.

CLI `task validate` and `task compile` use these same typed helpers.
`task compile TASK.json WEB-IR.json` requires the stable source IR and binds the
plan to its revision and semantic entity IDs; neither command starts Chrome.

CLI `ir validate`, `ir inspect`, and `ir continuity` use the corresponding
typed Web IR helpers. The detailed CLI `ir diff` projection remains the
default for local diagnostics; `ir diff --summary` selects the bounded
canonical `WebIrDiffResult` projection used by the protocol helpers.

A request must contain protocol version `1`, a non-empty operation name, and a
bounded request ID. A response contains one result or one structured error.

## Compatibility

Glass rejects unknown envelope fields. Additive fields must be optional. A
change to a required field or validation meaning requires a new schema version
and a migration note.

Protocol deadlines are limited to fifteen minutes. Each transport keeps its
own frame limits.

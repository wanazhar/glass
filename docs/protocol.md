# Glass protocol

Glass keeps MCP JSON-RPC framing at the transport boundary and uses the
versioned Glass envelope for operation semantics. The envelope is defined by
[`glass-protocol-v1.schema.json`](schema/glass-protocol-v1.schema.json) and by
the Rust `glass::protocol` module.

For an MCP tool call, the canonical mapping is:

| MCP field | Glass field |
| --- | --- |
| JSON-RPC `id` | `requestId` |
| `params.name` | `operation` with the `browser.` prefix |
| `params.arguments` | `payload` |
| daemon session/lease context | canonical `sessionId` / `mutationLease` fields are available to daemon-aware adapters; the current MCP bridge enforces the lease token in tool arguments |

The CLI and SDKs expose the same operation names and payload fields. MCP keeps
its JSON-RPC success/error framing, while the operation payloads use the same
bounded, versioned semantics. A request must use protocol version `1`, a
non-empty bounded operation name, and a bounded request identifier. Responses
carry exactly one result or structured error.

Unknown envelope fields are rejected. Additive contract fields must be
optional, and changes to required fields or validation meaning require a new
schema version and a migration note. Protocol deadlines are bounded to fifteen
minutes; transport-specific frame limits remain enforced by each transport.

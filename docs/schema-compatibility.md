# Schema compatibility

The transport-neutral operation mapping is in [Protocol](protocol.md).

Glass 0.2.x treats the versioned protocol and schema inventory as a compatibility
boundary. Existing action names and legacy result fields remain available.

## Rules

- Use camelCase at CLI JSON and MCP boundaries.
- Keep idiomatic Rust field names in the Rust API.
- Treat `expectedRevision` as optional. Its absence keeps the unguarded path.
- Include `executionId` in successful action results and bounded traces.
- Accept only documented finite verification predicates.
- Keep batch `mode` at `unguarded` by default. Require an initial revision
  for `fixed` and `chain`.
- Reject unknown action names, invalid types, and unbounded values.
- Reject incompatible persisted checkpoints, traces, knowledge snapshots, and
  replay bundles.

## Version policy

A patch release may fix defects, add optional fields, or add a capability that
clients can discover through negotiation.

A new required field, changed enum meaning, or changed validation meaning needs a
new schema version and a migration note.

Remove a field only after a documented deprecation period and compatibility
test.

Negotiate capability availability. Do not infer it from `glassVersion`.

The Rust library, CLI, MCP server, TypeScript client, and Python client use the
same argument names and result vocabulary.

Capability manifests retain the legacy boolean `capabilities` map and may add
the optional `capabilityStatuses` map. Statuses distinguish
`disabledByPolicy`, `availableUncertified`, and `blockedBySecurityGate`
without making the additive field required for older clients.

## Capability negotiation

MCP `initialize` returns a `glass` manifest that follows
[glass-capabilities-v1.schema.json](schema/glass-capabilities-v1.schema.json).

A client may request protocol and schema versions:

```json
{
  "protocolVersion": 1,
  "schemas": {
    "action": [1],
    "workflow": [1],
    "task": [1],
    "webIr": [1]
  }
}
```

Glass rejects unknown schemas, empty version lists, and requests without a
common supported version before it marks the MCP session ready.

If the client omits the Glass request, Glass still returns the manifest. The
client must inspect the manifest and policy-sensitive capability flags.

## Current contracts

- [protocol v1](schema/glass-protocol-v1.schema.json);
- [daemon recovery v1](schema/glass-daemon-recovery-v1.schema.json);
- [action v1](schema/glass-action-v1.schema.json);
- [policy error v1](schema/glass-policy-error-v1.schema.json);
- [session checkpoint v1](schema/glass-checkpoint-v1.schema.json);
- [workflow checkpoint v1](schema/glass-workflow-checkpoint-v1.schema.json);
- [workflow trace v1](schema/glass-workflow-trace-v1.schema.json);
- [semantic observation v1](schema/semantic-observation-v1.schema.json);
- [intent v1](schema/intent-resolution-v1.schema.json);
- [knowledge v1](schema/knowledge-v1.schema.json);
- [workflow v1](schema/workflow-v1.schema.json);
- reliability scenario, fixture, and replay v1;
- public read-only adapter inventory v1; and
- browser-free Task Protocol v1 and draft Web IR v1 canonical contracts
  described in [the protocol mapping](protocol.md).

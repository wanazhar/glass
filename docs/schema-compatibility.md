# Schema compatibility

Glass 0.2.x treats the versioned protocol and schema inventory as a stable
compatibility boundary. Existing action names and legacy result fields remain
available; new fields use camelCase at the CLI JSON and MCP boundaries while
Rust field names remain idiomatic.

Compatibility rules:

- `expectedRevision` is optional on every supported mutation. Omitting it keeps
  the unguarded compatibility path.
- `executionId` is present on successful action outcomes and bounded traces.
- `verify` accepts only documented finite predicate forms and bounded
  deadlines.
- Batch `mode` defaults to `unguarded`; `fixed` and `chain` require an
  explicit initial `expectedRevision`.
- Unknown fields are ignored where the existing parser is permissive; unknown
  action names, invalid types, and unbounded values are rejected.

## Release policy

- Patch releases may fix implementation defects, add optional fields, and add
  capabilities that are discoverable through negotiation.
- New required fields, changed enum meaning, or changed validation semantics
  require a new schema version and a migration note.
- Removed fields require a deprecation period and a compatibility test.
- Capability availability is negotiated independently from the binary version;
  a client must not infer support from `glassVersion` alone.
- Persisted checkpoints, traces, knowledge snapshots, and replay bundles must
  retain their schema version and reject incompatible future data safely.

The Rust library, CLI, MCP server, TypeScript client, and Python client use the
same argument names and result vocabulary. Removing or renaming a published
field requires a major contract decision rather than a patch release.

## Glass capability negotiation

MCP `initialize` returns a `glass` manifest conforming to
[glass-capabilities-v1.schema.json](schema/glass-capabilities-v1.schema.json).
The manifest is the authoritative inventory of schema versions, optional
capabilities, platform/browser constraints, and the active policy. A client
may request a Glass protocol and schema set in `initialize.params.glass`:

```json
{
  "protocolVersion": 1,
  "schemas": {
    "action": [1],
    "workflow": [1]
  }
}
```

The server rejects unknown schemas, empty version lists, and requests with no
supported version in common before entering the ready state. Omitting the
Glass request preserves MCP compatibility and still returns the manifest.
Unsupported capabilities are never inferred from the binary version; clients
must inspect the negotiated manifest and the policy-sensitive booleans.

The current machine-readable contract set is:

- [action v1](schema/glass-action-v1.schema.json)
- [policy error v1](schema/glass-policy-error-v1.schema.json)
- [session checkpoint v1](schema/glass-checkpoint-v1.schema.json)
- [workflow checkpoint v1](schema/glass-workflow-checkpoint-v1.schema.json)
- [workflow trace v1](schema/glass-workflow-trace-v1.schema.json)
- [semantic observation v1](schema/semantic-observation-v1.schema.json)
- [intent v1](schema/intent-resolution-v1.schema.json)
- [knowledge v1](schema/knowledge-v1.schema.json)
- [workflow v1](schema/workflow-v1.schema.json)
- [reliability scenario, fixture, and replay v1](reliability.md)

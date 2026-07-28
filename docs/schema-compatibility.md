# Schema compatibility

The `0.1.18` reliability contract is additive. Existing action names and
legacy result fields remain available; new fields use camelCase at the CLI JSON
and MCP boundaries while Rust field names remain idiomatic.

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

---
id: ship-input-001
status: complete
depends-on: [ship-terminal-001, ship-security-001]
scope: ship-next-20 item 19 and security UI integration
---
# Delivery: input

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Contract

Use the existing key reader and owned modal body; never add a second stdin
consumer. Bind a scoped host input callback during interactive turns. Headless,
serve and ACP without an interactive callback return a bounded actionable
input-required refusal. Do not advertise modes without a real handler.

The MCP bridge sends a nested `request_input` message to the Rust host, receives
responses, and retries the original tool request. Preserve the original request
parameters; assign a fresh RPC id. `inputRequests` and `inputResponses` are maps
whose keys match. Echo opaque `requestState` exactly; do not persist it.
Bound rounds, schema size, fields, response size and waiting time. Reject
unsupported schemas rather than returning invalid answers.

## Layout

```
[MCP server · requested input]
[message / HTTPS destination host]
[field or selection currently focused]
[review response · submit / decline / cancel]
------------------------------------------------------
> existing composer draft
```

At 20x8 keep a focused field and navigation action visible. Form values remain
private to the callback and bridge, with no tool-output echo or transcript/session
logging. Forms reject credential fields. URL mode shows the target host and
requires consent; never navigate automatically. Review before submit.

Wire the security foundation's session exception approval and permission/capability
inspection APIs into existing modals without changing the unrestricted default.
Update doctor to show configured profiles and enforced extension restrictions.

## Sources and verification

Wire contracts follow the official
[elicitation specification](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/client/elicitation.mdx)
and [multi round-trip requests](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/basic/patterns/mrtr.mdx).

Use actual bridge mocks for form, titled/multiple enums, URL, decline/cancel,
headless refusal, private-value absence, fresh RPC ids and exact state echo.
Verify all modals with actual VT cells at 40x24 and 20x8.

## Final verification

Completed in root source checkpoint `1890e59`; see
[final evidence](../reviews/ship-final-001-01.md). Independent review blockers were
fixed before the clean release run. All 40 smoke suites, 571 workspace tests,
strict Clippy and performance gates passed, with real PTYs at 40x24 and 20x8.

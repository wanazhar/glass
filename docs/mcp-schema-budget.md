# MCP schema and context budget

Glass treats MCP discovery as a measurable context cost. This guide defines the
measurement, current source-line evidence, growth rules, and reduction options.
It is not the tool reference; use the [complete MCP catalog](mcp-tools.md) for
purpose and authority, and the live negotiated `tools/list` result for exact
schemas supported by an installed server.

## Current 0.3.9 development-source measurement

On the current checkout, `target/debug/glass` reports:

| Metric | Measured value |
|---|---:|
| Negotiated tools | 299 |
| Serialized `tools` array | 146,421 UTF-8 bytes |
| Four-bytes-per-token estimate | 36,606 tokens |
| JSON-RPC framing | excluded |

This is a reproducible local measurement, not a guarantee for another commit,
client tokenizer, capability agreement, or future release. The tool count is
pinned independently by the client-conformance fixture and documentation
coverage gate.

The development surface is larger than the earlier browser-only inventory because it
also exposes the Development Runtime, semantic execution, memory, workspace,
backend, replay, and recovery contracts. Older counts and byte totals must not
be carried forward as current evidence.

## Reproduce the measurement

Build the exact executable, then run the browser-free probe:

```console
cargo build --package glass-dev --all-features --locked
GLASS_BINARY_PATH=target/debug/glass node benchmarks/schema-scoreboard.mjs
```

The probe starts `glass --mcp`, completes initialization and the initialized
notification, requests `tools/list`, serializes only the returned `tools`
array, counts UTF-8 bytes, and divides by four for a conservative model-agnostic
estimate. It does not start Chrome.

Record the commit, binary profile, tool count, bytes, and methodology together.
Do not compare a full JSON-RPC response with a bare array, or a pre-negotiation
catalog with the effective tool list.

## Budget and acceptance

The independently installable `glass-browser` product retains the 64 KiB
review ceiling for its browser-only catalog. The full `glass` development
product uses a separate 160 KiB ceiling for the merged browser and resident
workspace catalog. This is a regression alarm, not permission to consume the
remaining space. Any increase must include the before/after scoreboard and
explain why a new public tool is preferable to an existing typed verb or a
namespaced resource.

The increase from the published 0.3.4 measurement of 129,444 bytes to the
current 146,421-byte development inventory covers explicit trust, autonomous
task, measured experiment, debugger inspection, governed-kernel operations,
and the governed Agent composer/runtime setup routes.
`glass` exposes the same typed resident services used by Pi, the TUI, CLI, and
daemon, while `glass-browser` remains the compact browser-only product. A future
capability-scoped discovery protocol can reduce per-client context without
hiding tools from clients that require the complete workspace.

A change is rejected when it:

- exceeds the applicable 64 KiB browser or 160 KiB development ceiling without
  an accepted compatibility design and release note;
- duplicates an existing operation under a new name;
- adds unbounded strings, arrays, maps, or recursive input;
- embeds large examples or result schemas in discovery descriptions;
- exposes raw CDP as a convenience path around policy; or
- changes a required field without schema negotiation and migration guidance.

Tool count alone is not the budget. A single deeply nested schema can cost more
than several no-argument tools, so review total bytes and the per-tool list from
the scoreboard.

## Current high-cost schemas

The current measurement identifies these larger input schemas:

| Tool | Input-schema bytes | Reason |
|---|---:|---|
| `extractStructured` | 1,576 | typed extraction fields, sources, and bounds |
| `resolveIntentWithKnowledge` | 1,133 | intent plus scoped knowledge controls |
| `executeIntent` | 826 | revision-bound executable intent |
| `executeTask` | 767 | validated Task Protocol execution controls |
| `resolveIntent` | 695 | structured intent resolution |
| `actAndVerify` | 691 | action plus explicit postcondition evidence |
| `glass.file.grep` | 664 | bounded search scope and result controls |

These sizes are diagnostic priorities, not automatic defects. Simplification
must preserve validation, bounds, authority, and compatibility; moving required
constraints out of schema and into undocumented prose is not an optimization.

## Design rules

1. Prefer stable verbs with typed variants over one tool per locator or mode.
2. Keep structured observation the default and large DOM, screenshot, PDF,
   evaluated, and form-value payloads explicit.
3. Bound every collection, string, nesting depth, response, and deadline in the
   canonical server contract.
4. Put operational explanation in this documentation and concise actionable
   descriptions in discovery.
5. Negotiate optional or experimental capabilities; do not advertise a method
   as usable when the effective agreement disables it.
6. Keep project and browser mutations visibly distinct and preserve leases,
   actor authority, policy, and revision guards.

## Client strategy

Clients should cache the discovery result only for the initialized connection.
Do not assume that a fixture, SDK method list, or previous server has the same
effective agreement. Inspect `glassAgreement`, then use `tools/list` and
capability/schema checks for optional behavior. A reconnect creates a new
agreement and requires fresh discovery.

The current `glass` executable advertises the 299-tool merged catalog in the
reproducible scoreboard; `glass-browser` retains its independently measured
browser catalog. The effective capability
agreement determines which optional operations are usable. Context reduction
must use an explicitly versioned future negotiation mechanism; clients must not
silently drop schemas the server advertises.

## Release maintenance

Whenever a tool or input schema changes:

1. update the server registry and canonical result type;
2. update `client-conformance-v1.json` and [the tool catalog](mcp-tools.md);
3. rebuild and run the scoreboard against the exact binary;
4. run `python3 scripts/check-documentation-coverage.py`;
5. update the measured table and release evidence when the numbers change; and
6. verify TypeScript/Python negotiation smoke tests against the matching
   executable.

An unexplained count or byte drift fails the documentation/release review even
when the server still compiles.

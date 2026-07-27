id: schema-020
scope: cross-interface schema compatibility
status: done
depends-on: [clients-020, batch-020]

## objective

Document and test additive compatibility rules for Rust, CLI JSON, MCP,
TypeScript, and Python surfaces.

## path

- `docs/schema-compatibility.md`
- `docs/INDEX.md`
- `docs/mcp-schema-budget.md`
- `src/mcp/resources.rs`
- `src/mcp/server.rs`
- GitHub issue #20

## verification

- MCP exposes the action contract as a static resource.
- Existing resource and compact-result tests remain green.
- No remote push, tag, or publication occurs.

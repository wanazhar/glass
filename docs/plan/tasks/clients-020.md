id: clients-020
scope: cross-interface reliability helpers
status: done
depends-on: [verify-020, batch-020]

## objective

Align the maintained TypeScript and Python MCP clients with the Rust server's
revision guards, bounded verification predicates, and batch modes.

## path

- `clients/typescript/src/index.ts`
- `clients/typescript/README.md`
- `clients/python/glass_client.py`
- `clients/python/README.md`
- GitHub issue #20

## verification

- Client helper names and JSON fields match the MCP contract.
- No browser-runtime dependency is added.
- No remote push, tag, or publication occurs.

id: batch-020
scope: revision-safe batch execution
status: done
depends-on: [mutation-020]

## objective

Give ordered batches explicit `fixed`, `chain`, and `unguarded` revision
policies with bounded standard result metadata, while preserving the legacy
unguarded API and `atomic` target preflight option.

## context

- `docs/action-contract.md`
- `docs/cli.md`
- `docs/mcp.md`
- `docs/mcp-schema-budget.md`
- GitHub issue #20

## path

- `src/browser/session/types.rs`
- `src/browser/session/batch.rs`
- `src/cli/args.rs`
- `src/cli/runner.rs`
- `src/mcp/server.rs`

## verification

- MCP parser tests cover all batch modes and the initial revision guard.
- Fixed and chain modes reject missing initial revisions.
- `cargo fmt --all -- --check` passes.
- `cargo check --all-targets` passes.
- No remote push, tag, or publication occurs.

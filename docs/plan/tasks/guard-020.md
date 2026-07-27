id: guard-020
scope: revision-safe mutation coverage
status: done
depends-on: [contract-020]

## objective

Extend the optional expected-revision guard across the currently supported
targeted mutations: popup clicks, double-click, clear, check, uncheck, select,
and scroll. Preserve unguarded compatibility methods and expose the same
fields through Rust, CLI, and MCP.

## context

- `docs/action-contract.md`
- `docs/actions.md`
- `docs/cli.md`
- `docs/mcp.md`
- GitHub issue #20

## path

- `src/browser/session/action.rs`
- `src/browser/session/popup.rs`
- `src/cli/args.rs`
- `src/cli/runner.rs`
- `src/mcp/server.rs`

## verification

- MCP schemas and parsers retain `expectedRevision` for every guarded action.
- Legacy methods remain available and delegate to the unguarded form.
- `cargo fmt --all -- --check` passes.
- `cargo test --all-targets` passes.
- No remote push, tag, or publication occurs.

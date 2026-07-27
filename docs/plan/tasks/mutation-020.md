id: mutation-020
scope: revision-safe keyboard, drag, and upload coverage
status: done
depends-on: [guard-020]

## objective

Complete optional expected-revision coverage for the remaining targeted
mutations: keyboard press/down/up and shortcuts, drag, and file upload.
Preserve unguarded compatibility methods and expose one matching contract in
Rust, CLI, and MCP.

## context

- `docs/action-contract.md`
- `docs/actions.md`
- `docs/cli.md`
- `docs/mcp.md`
- GitHub issue #20

## path

- `src/browser/session/action.rs`
- `src/cli/args.rs`
- `src/cli/runner.rs`
- `src/mcp/server.rs`

## verification

- MCP parser and schema tests cover every guarded mutation.
- CLI argument tests preserve legacy invocations with no guard.
- `cargo fmt --all -- --check` passes.
- `cargo test --all-targets` passes.
- No remote push, tag, or publication occurs.

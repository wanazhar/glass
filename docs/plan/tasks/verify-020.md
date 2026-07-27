id: verify-020
scope: bounded verification predicates
status: done
depends-on: [failure-020]

## objective

Provide a bounded predicate evaluator for URL, title, visibility, text,
popup, dialog, download, revision, and Boolean composition, with explicit
deadlines and no arbitrary JavaScript.

## context

- `docs/action-contract.md`
- `docs/cli.md`
- `docs/mcp.md`
- GitHub issue #20

## path

- `src/browser/session/types.rs`
- `src/browser/session/wait.rs`
- `src/browser/session/download.rs`
- `src/browser/session/mod.rs`
- `src/browser/session/tests.rs`
- `src/cli/args.rs`
- `src/cli/runner.rs`
- `src/mcp/server.rs`

## verification

- Predicate composition is depth- and fan-out-bounded.
- MCP parser tests cover nested predicates and bounded timeout parsing.
- `cargo check --all-targets` passes.
- No remote push, tag, or publication occurs.

id: contract-020
scope: canonical action contract foundation
status: done
depends-on: []

## objective

Introduce the first `0.1.18` reliability-runtime slice: document the shared
action envelope, assign bounded execution IDs to existing action result types,
and establish the internal request boundary used to converge guarded and
unguarded actions without removing compatibility APIs.

## context

- `docs/action-contract.md`
- `docs/actions.md`
- `docs/actions.md`
- `docs/architecture/automation.md`
- `docs/INDEX.md`
- GitHub issue #20

## path

- `docs/action-contract.md`
- `docs/INDEX.md`
- `docs/plan/tasks/contract-020.md`
- `src/browser/session/mod.rs`
- `src/browser/session/types.rs`
- `src/browser/session/action.rs`
- `src/browser/session/fill.rs`
- `src/browser/session/navigate.rs`
- `src/browser/session/popup.rs`
- `src/browser/session/tests.rs`
- `src/mcp/server.rs`

## verification

- Unit tests prove execution IDs are unique within a session and serialize as
  `executionId`.
- Existing action, MCP, and browser smoke tests continue to pass.
- `cargo fmt --all -- --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- No remote push, tag, or publication occurs.

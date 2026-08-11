# security-035-001: explicit workspace trust boundary

Status: Complete and verified locally

## Outcome

Opening an unfamiliar project performs bounded static inspection only. No
repository-controlled command runs and no project skill enters privileged Pi
instructions until a local user chooses `TrustedOnce` or an identity-bound
`TrustedProject` record is found in Glass-owned storage.

## Contract

- Add `Untrusted`, `TrustedOnce`, and `TrustedProject` states.
- Bind persisted trust to canonical root plus repository/filesystem identity,
  version the store, keep it outside repository control, and fail closed after
  identity replacement.
- Separate user-global skills, trusted-project skills, and visible untrusted
  project instructions.
- Classify every project configuration item as static, agent context, or
  executable and expose exact source/command inspection.
- Require trust for every shell-backed project tool regardless of declared
  mutation metadata; preserve mutation metadata only for post-trust UX.
- Do not register configured tests or execute hooks while untrusted. Gate
  configured commands, LSP/DAP overrides, setup, agents, kernels,
  experiments/worktrees, and executable project paths at the authoritative
  router/workspace boundary.
- Expose read-only trust status/inspection through the shared tool contract.
  Trust mutation requires a local-human authority path in CLI/TUI; MCP,
  daemon, embedded agents, kernels, and untrusted configuration cannot upgrade
  themselves.
- Present the same trust decision and inspection on desktop, compact, and
  phone TUI layouts before executable activation.
- `TrustedOnce` is process-lifetime state only. `TrustedProject` persists in
  the Glass-owned store. Daemon reconnect preserves but cannot elevate state.
  Experiment worktrees require an explicit identity/policy decision.

## Required tests

1. Untrusted open does not execute `workspace.opened`.
2. Untrusted open/tool calls do not run project tools, tests, LSP, or DAP.
3. Project skills are visible but absent from privileged agent instructions.
4. `TrustedOnce` is absent on a later open.
5. `TrustedProject` is recovered only from the external store.
6. Replaced identity causes trust re-evaluation.
7. CLI, TUI, MCP, daemon, and agent/router paths return the same state/denial.
8. Daemon reconnect cannot elevate trust.
9. Experiment/worktree inheritance follows an explicit fail-closed policy.
10. Repository config cannot declare itself trusted.

## Verification

```console
cargo test -p glass-dev trust
cargo test -p glass-dev customization
cargo test -p glass-dev mcp
cargo test -p glass-dev daemon
cargo fmt --all -- --check
cargo clippy -p glass-dev --all-targets -- -D warnings
```

All ten required security behaviors are covered by trust, workspace, MCP,
daemon, experiment-policy, TUI, and black-box CLI tests. The complete
`glass-dev` suite and strict package Clippy pass at this checkpoint.

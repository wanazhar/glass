---
id: issue60-p0-governance-001
scope: glass-dev/trust-mutation-governance
status: ready
depends-on: []
---

# Issue #60 P0: effective trust and mutation governance

## Objective

Resolve or reproduce the current status of findings F37, F41, F55, F59, F91,
F92, and F95. Make advertised tool availability, trust inspection, and actual
execution agree. Require both mutation authority and confirmation at every
public mutation boundary, including direct library delegation. Align kernel
capability metadata/enforcement and Git status/protected-branch handling.
Preserve the F92 root-level `x-glass-mutating` placement and add regression
coverage for it. If F37 is already fixed, add evidence that demonstrates the
shared authorization route rather than changing correct code.

## Context

- `docs/workspace-trust.md`
- `docs/customization-governance.md`
- `docs/harness-architecture.md`
- `docs/mcp-tools.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), findings F37/F41/F55/F59/F91/F92/F95
- Source reports [#47](https://github.com/wanazhar/glass/issues/47), [#49](https://github.com/wanazhar/glass/issues/49), and [#50](https://github.com/wanazhar/glass/issues/50)

## Contract

- Inspection and availability use the same effective trust and authorization
  decision as the execution router, including process-local unrestricted mode.
- Mutating library, CLI, TUI, MCP, daemon, and harness paths require the
  documented authority and confirmation pair; no adapter is the sole guard.
- Public custom shell calls carry the pair explicitly; raw project, Git, and
  kernel mutation methods are crate-private behind governed workspace tools.
- Kernel and Git metadata describe the effective operation and protected
  branch policy used by enforcement, including pushes checked against the
  repository's queried default branch at the Git service boundary. Each push
  names exactly one source and destination ref so Git push configuration cannot
  add unchecked branches.
- MCP advertises `x-glass-mutating` at the root of each tool input schema.
- Keep the existing trust and mutation failure modes explicit and fail closed.

## Path

- `crates/glass-dev/src/workspace.rs`
- `crates/glass-dev/src/customization.rs`
- `crates/glass-dev/src/tools.rs`
- `crates/glass-dev/src/mcp.rs`
- `crates/glass-dev/src/cli.rs`
- `crates/glass-dev/src/external_agents.rs`
- `crates/glass-dev/src/kernels.rs`
- `crates/glass-dev/src/git.rs`
- `crates/glass-dev/src/github.rs`
- Relevant trust, policy, MCP, and harness docs

## Verification

- Add focused regressions for each changed decision and each public boundary
  involved in the fix.
- Exercise the real workspace/router path for trust inspection versus tool
  listing and execution.
- Cover direct library mutation with each authority/confirmation combination.
- Confirm schema metadata is root-level and `_glass` remains an ordinary
  parameter object.
- Record any cross-platform Git limitation without weakening the contract.

---
id: issue60-p0-governance-001
scope: glass-dev/trust-mutation-governance
status: done
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
- Raw agent, task, test, process, LSP, debugger, browser, and experiment services are
  internal implementation details. Public callers use
  `DevelopmentWorkspace::tool_descriptors` and `execute_tool`, so trust,
  revision, turn-mode, mutation, and confirmation checks share one boundary.
  Public workspace accessors do not expose mutable raw services.
- Todo persistence, workspace knowledge mutation, trust-store writes, local
  trust activation hooks, and unrestricted-mode activation cannot be reached
  through context-less public mutation methods.
- Public custom shell calls carry the pair explicitly; raw project, Git, and
  kernel mutation methods are crate-private behind governed workspace tools.
- Pi slash dispatch can create or restart resident sessions and execute native
  commands, so descriptors classify it as mutating, untrusted workspaces cannot
  call it, and the TUI queues it behind its mutation confirmation flow.
- Kernel start schemas expose the explicit `mutationAuthority` capability
  grant, while the router independently enforces both authorization factors.
- Kernel and Git metadata describe the effective operation and protected
  branch policy used by enforcement, including pushes checked against the
  actual destination repository's queried default branch at the Git service
  boundary. Unsupported or ambiguous push remotes fail closed. Each push names
  exactly one source and destination ref so Git push configuration cannot add
  unchecked branches.
- MCP advertises `x-glass-mutating` at the root of each tool input schema.
- Keep the existing trust and mutation failure modes explicit and fail closed.

## Path

- `crates/glass-dev/src/workspace.rs`
- `crates/glass-dev/src/customization.rs`
- `crates/glass-dev/src/tools.rs`
- `crates/glass-dev/src/mcp.rs`
- `crates/glass-dev/src/cli.rs`
- `crates/glass-dev/src/external_agents.rs`
- `crates/glass-dev/src/agents.rs`
- `crates/glass-dev/src/browser.rs`
- `crates/glass-dev/src/debugger.rs`
- `crates/glass-dev/src/development/agent.rs`
- `crates/glass-dev/src/development/language.rs`
- `crates/glass-dev/src/experiments.rs`
- `crates/glass-dev/src/lsp.rs`
- `crates/glass-dev/src/tasks.rs`
- `crates/glass-dev/src/testing.rs`
- `crates/glass-dev/src/todos.rs`
- `crates/glass-dev/src/trust.rs`
- `crates/glass-dev/src/kernels.rs`
- `crates/glass-dev/src/git.rs`
- `crates/glass-dev/src/github.rs`
- Relevant trust, policy, MCP, and harness docs

## Verification

- `cargo check -p glass-dev --lib --bins --locked` passed.
- `cargo test -p glass-dev every_public_service_mutation_route_requires_both_authorization_factors --locked` passed (1 test).
- `cargo test -p glass-dev --lib --locked slash` passed (4 tests), covering
  mutation denial, untrusted workspace denial, and the TUI confirmation queue
  for `glass.agent.slash`.
- `cargo test -p glass-dev --doc --locked` passed (12 doctests, including public-API compile-fail fences).
- `cargo test -p glass-dev --test development_runtime --locked` passed (4 tests), including trusted MCP listing and one-factor mutation rejection.
- Independent review of the complete task diff passed with no findings. The
  TUI regression exercises submission and denial; approval-submit and
  asynchronous-error cleanup were verified by source inspection.
- `cargo build -p glass-browser --bin glass-native-content-worker --locked` passed; this binary is required by one native-browser regression.
- `cargo test -p glass-dev --lib --locked` passed (395 tests).
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Scoped Clippy passed with `-A clippy::large_enum_variant`. The unmodified `ResidentBrowserSession` enum at `crates/glass-dev/src/browser.rs:154` still causes the strict all-targets Clippy command to fail; the broader `glass-browser` dependency also has existing lint failures.

---
id: core-provider-controls-001
scope: issue-63-core-provider-controls
status: in-progress
depends-on: [issue63-mixed-tools-001]
---

# Core-owned provider controls in Glass TUI

## Objective

Make provider authentication and model selection in the Glass TUI respect
Glass Core's ownership. Login hands the terminal to Core's own interactive
login flow; model list/selection use the negotiated Core session APIs. Keep
Pi-specific setup and labels accurate only in explicit legacy mode.

## Context

- `docs/plan/analysis/core-provider-controls.md`
- `docs/plan/analysis/mixed-runtime-tools.md`
- `docs/glass-core-harness.md`
- `docs/development-runtime.md`
- `AGENTS.md`

## Scope

- `crates/glass-dev/src/tui/command.rs`
- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `crates/glass-dev/src/core_runtime.rs` and resident adapter tests only as
  needed to prove negotiated model requests
- `crates/glass-dev/tests/` fake runtime/PTY fixture only if required
- Current Core harness/runtime/CLI documentation

Do not redesign the TUI, duplicate provider credentials/configuration in
Glass, modify provider implementations, or alter Glass/Core ownership for
other tools.

## Acceptance

- [ ] `:agent setup login` suspends Glass and launches the resolved
      `glass-core` interactive UI with inherited terminal streams and current
      workspace directory; no secret is passed or stored by Glass.
- [ ] Return from Core resumes Glass and refreshes readiness; child resolution,
      launch, and terminal-resume errors are surfaced truthfully.
- [ ] The current resident process/session is not automatically restarted after
      Core changes configuration. Tell the user that a new conversation uses
      the newly selected Core settings and leave the existing session intact.
- [ ] Default Core action catalog, doctor, setup and auth remediation use
      Core-specific wording. Explicit `GLASS_AGENT_HARNESS=pi` keeps accurate
      Pi setup/login labels and the current Pi login behavior.
- [ ] Model list and selection target the active session and use Core's
      negotiated `models/list` and `session/configure` methods. No provider
      credentials or API keys pass through Glass configuration.
- [ ] Tests cover protocol method/parameter mapping, no-active-session and
      unsupported/error responses, Core/Pi command behavior, and a fake
      executable terminal handoff (PTY if the environment permits).
- [ ] Update current docs and link this plan from `docs/plan/README.md`; keep
      historical release/evidence records unchanged.
- [ ] Scoped Rust tests, `cargo fmt --all -- --check`, strict scoped Clippy,
      docs depth/coverage/release-truth/version validators, and diff review
      pass. Reuse
      `/home/ubuntu/work/glass/.worktrees/issue63-mixed-tools-001/target` for
      Cargo checks; do not create or clean a target tree.
- [ ] Commit locally with a focused Conventional Commit. Do not push, release,
      globally install, restart services, or close issue #63.

## Verification record

Pending implementation and review.

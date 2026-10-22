---
id: core-provider-controls-001
scope: issue-63-core-provider-controls
status: complete
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

- [x] `:agent setup login` suspends Glass and launches the resolved
      `glass-core` interactive UI with inherited terminal streams and current
      workspace directory; no secret is passed or stored by Glass.
- [x] Return from Core resumes Glass and refreshes readiness; child resolution,
      launch, and terminal-resume errors are surfaced truthfully.
- [x] The current resident process/session is not automatically restarted after
      Core changes configuration. `:agent new` is not a fresh process; tell the
      user to start a new independent resident agent/runtime process (such as
      `:agent spawn`) for new settings and leave the existing session intact.
- [x] Default Core action catalog, doctor, setup and auth remediation use
      Core-specific wording. Explicit `GLASS_AGENT_HARNESS=pi` keeps accurate
      Pi setup/login labels and the current Pi login behavior.
- [x] Model list and selection target the selected or active session and use Core's
      negotiated `models/list` and `session/configure` methods. No provider
      credentials or API keys pass through Glass configuration.
- [x] Tests cover protocol method/parameter mapping, no-active-session and
      unsupported/error responses, Core/Pi command behavior, and a fake
      executable terminal handoff (PTY if the environment permits).
- [x] Update current docs and link this plan from `docs/plan/README.md`; keep
      historical release/evidence records unchanged.
- [x] Scoped Rust tests, `cargo fmt --all -- --check`, strict scoped Clippy,
      docs depth/coverage/release-truth/version validators, and diff review
      pass. Reuse
      `/home/ubuntu/work/glass/.worktrees/issue63-mixed-tools-001/target` for
      Cargo checks; do not create or clean a target tree.
- [x] Commit locally with a focused Conventional Commit. Do not push, release,
      globally install, restart services, or close issue #63.

## Verification record

Implementation is local on `task/core-provider-controls-001`; no push,
release, global install, service restart, or issue closure was performed. Core
login resolves and canonicalizes the configured Core executable, suspends and
resumes Glass, launches with `Command::status()` (inherited standard streams)
in the workspace with no arguments, and refreshes readiness without restarting
the active resident process. Newly selected Core settings require a newly
spawned resident agent; `:agent new` remains in the existing process. The
legacy Pi login path remains selected only by `GLASS_AGENT_HARNESS=pi`.

Verification passed using the shared target at
`/home/ubuntu/work/glass/.worktrees/issue63-mixed-tools-001/target`:

- `cargo test -p glass-dev --lib core_ --locked` — 23 passed, including the
  fake executable cwd/no-arguments handoff, Core/Pi catalog behavior, and the
  background-action refusal and Core model response operation mapping.
- `cargo test -p glass-dev --lib model_listing_without_an_active_session_returns_actionable_conflict --locked` — passed.
- `cargo test -p glass-dev --lib selected_model_agent_precedes_workspace_first_active_agent --locked` — passed; the selected session wins over the workspace-first active fallback.
- `cargo test -p glass-dev --lib core_model_response_surfaces_provider_and_model_labels --locked` — passed; provider/model catalog labels are projected into the Agent conversation.
- `cargo test -p glass-dev --lib core_model_list_uses_the_selected_session_service_request --locked` — passed; the selected agent ID is included in the model-list request.
- `cargo test -p glass-dev --lib agent_chrome_and_session_picker_and_context_chips --locked` — passed.
- `cargo test -p glass-dev --lib surface_action_menu_is_discoverable_and_runs_or_prefills --locked` — passed.
- `cargo clippy -p glass-dev --lib --locked -- -D warnings` — passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- `python3 scripts/check-documentation-coverage.py --glass /home/ubuntu/work/glass/target/debug/glass --glass-browser /home/ubuntu/work/glass/target/debug/glass-browser` — passed (1,791 Markdown files).
- `python3 scripts/check-documentation-depth.py`, `python3 scripts/check-release-documentation.py`, and `python3 scripts/check-version-sync.py` — passed.

The handoff used a fake executable process fixture rather than a PTY; it
verified that no arguments are passed and the child runs from the workspace.

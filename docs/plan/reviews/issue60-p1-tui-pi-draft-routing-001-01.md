# Independent review: TUI-1 Pi draft routing

Reviewed implementation commit `4e7ee81213e16ed7af1e9ab1460b0f1a66a3cde0` against parent `ee502d05b95e866cd463560ca965e7f463ab60e7`, including the task contract, Development TUI contract, source diff, and related tests.

## Verdict: BLOCKED

### P2 — Blocking: denying a native command loses the existing composer draft

`crates/glass-dev/src/tui/state.rs:4003-4014` copies the existing composer input into `raw`, then clears `composer_input` and resets `composer_cursor` before opening and submitting the slash modal. For a native mutating command such as `/new`, `submit_pi_command` closes the modal and creates a confirmation (`state.rs:1808-1818`). `deny_confirmation` (`state.rs:4678-4687`) clears the pending command metadata but does not restore the composer text or cursor. Thus, entering `/new keep this draft` directly in the composer, pressing Enter, then denying with Esc returns to an empty composer at cursor zero instead of preserving the command for editing or retry.

This violates the task contract requiring that a slash command submitted from an existing composer draft use the shared route and that denying a pending mutation confirmation return to the same draft (`docs/plan/tasks/issue60-p1-tui-pi-draft-routing-001.md:32-35`). The Development TUI contract states the same draft-and-cursor preservation requirement (`docs/architecture/development-tui.md:319-323`). Preserve and restore the pre-submit composer text and cursor on denial (and on any submission failure after the early clear).

The existing `pi_slash_command_waits_for_mutation_confirmation` test (`state.rs:10829-10865`) covers opening the separate modal while a different composer draft exists. It does not cover entering the slash command as the existing composer draft. `composer_slash_compact_routes_to_native_pi_command` (`state.rs:13278-13301`) checks dispatch but not confirmation denial or draft restoration.

## Other reviewed paths

No additional blocking defect was found in the requested review scope. The modal open/Esc path retains the composer draft and cursor; typed modal input, leading-slash paste, and composer slash submission share the resolver; the input path enforces the 4 KiB bound and preserves multiline paste; and the legacy `/stats`, `/sessions`, `/think`, and `/todo` routes remain represented in the resolver/tests. F3 `:` routing remains separate from the Pi slash path.

## Checks

- `git diff --check ee502d05 4e7ee812` — passed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib pi_ --locked` — terminated with exit code 143 while compiling `glass-dev`; the test binary did not run, so this provides no test result.

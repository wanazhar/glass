# Independent review 02: TUI-1 Pi draft routing

Reviewed implementation commit `138c6c20cb16ea345fcc724cec5b564544004e4d` against parent `26a8651025e70f29022a21a3581c38d8ecc8a1b7`. Scope included the task contract, Development TUI contract, follow-up diff, and relevant state and regression-test paths.

## Verdict: BLOCKED

### P2 — Blocking: successful mode-only and Tasks routes discard composer-origin input

`submit_composer_slash` saves the input and cursor in `pi_command_composer_origin`, then clears the composer before dispatch (`crates/glass-dev/src/tui/state.rs:4057-4070`). The successful `/ask` route with no arguments closes the modal, changes the mode, clears the saved origin, and returns without restoring its text or cursor (`state.rs:1781-1788`). The `/todo` route likewise closes the modal, clears the saved origin, and switches to Tasks without restoring the input (`state.rs:1803-1807`).

Repro from the Agent composer: enter `/ask` and press Enter; the TUI returns to the composer in Ask mode with an empty input. Enter `/todo` and press Enter; the TUI moves to Tasks with the composer input empty, despite setting status to “composer draft preserved.” Both routes lose the submitted text and its original cursor position.

The task contract explicitly requires `/ask` without arguments to return to the composer with the draft intact and `/todo` to open Tasks while retaining the composer draft (`docs/plan/tasks/issue60-p1-tui-pi-draft-routing-001.md:46-51`). The new tests cover these routes through a separate modal/pasted command while an unrelated composer draft remains (`state.rs:11140-11158`, `11285-11303`); they do not cover the direct composer-origin route that clears the input before dispatch. Restore the saved composer origin on these successful routes before clearing that snapshot.

## Re-review of review-01 blocker

The prior P2 is fixed: native command denial now restores the saved text and cursor through `restore_pi_composer_origin`, and `composer_native_slash_denial_restores_exact_command_and_cursor` covers direct `/new` submission and denial (`state.rs:1606-1621`, `4746-4757`, `10939-10972`). The new submit-failure test and Trust redirect test also cover restoration on their respective paths. No additional blocker was found in those paths.

## Checks

- `git diff --check 26a86510 138c6c20` — passed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_ --locked` — started compiling `glass-browser`, then was interrupted by the reviewer before test execution to avoid extending the review build. No test result is claimed; the task record reports the focused remediation tests passing.

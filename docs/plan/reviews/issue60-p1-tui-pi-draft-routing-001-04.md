# Independent review 04: TUI-1 Pi draft routing

Reviewed implementation commit `9a2277b41d972efbc06fb7e125d0e35ced4e28a0`
against parent `ed8d7ac4`. Scope covered the review-03 remediation, the
remained-blocking contract surface, and the prior findings from reviews 01–03.
No implementation files were changed.

## Verdict: PASS

## Findings

**P1/P2 blocking: none.**

- The Tasks route clears composer focus unconditionally after closing the
  modal and restoring the snapshot
  (`crates/glass-dev/src/tui/state.rs:1803-1810`). This is the correct shape
  for review-03's finding: `restore_pi_composer_origin` returns early with
  `false` when there is no snapshot (`state.rs:1606-1609`), so the
  modal-origin/pasted path had no other way to drop focus, and the
  direct composer-origin path was already handled by the Trust/Tasks guard at
  `state.rs:1616-1618`. Placing the assignment after the restore call covers
  both paths without regressing draft text or cursor.
- With `composer_mode` false, `active_overlay` can no longer select
  `ActiveOverlay::Composer` (`crates/glass-dev/src/tui/overlay.rs:60-64`), so
  the Composer dispatch arm at `crates/glass-dev/src/tui/mod.rs:695-702` no
  longer preempts the Tasks navigation/activation arms at
  `mod.rs:994-1010`. `pi_command_mode` is already cleared by
  `close_pi_command_palette`, so `ActiveOverlay::PiSlashCommand` is not left
  selected either. This matches the contract wording at
  `docs/plan/tasks/issue60-p1-tui-pi-draft-routing-001.md:50-52` and
  `docs/architecture/development-tui.md:356-358`.
- Both regressions now assert the focus contract rather than the defect:
  `pasted_todo_opens_tasks_without_changing_composer_draft_or_cursor`
  (`state.rs:11340-11365`) covers the modal-origin path and
  `composer_todo_route_restores_exact_command_and_cursor_on_tasks`
  (`state.rs:11188-11213`) covers the direct composer-origin path. Each
  asserts `!composer_mode`, `!pi_command_mode`,
  `active_overlay != Composer`, and the retained text/cursor. The review-03
  report noted the pasted test previously asserted the conflicting
  `composer_mode` true; that assertion is now inverted, so the regression is
  encoded correctly.

## Re-review of prior findings

Review-01 (denial lost the draft) remains fixed: `restore_pi_composer_origin`
is called from `deny_confirmation` (`state.rs:4746-4757`) and covered by
`composer_native_slash_denial_restores_exact_command_and_cursor`
(`state.rs:10942-10970`). Review-02 (mode-only and Tasks routes discarded the
submitted composer-origin input) remains fixed: the `ComposerMode` branch
restores the snapshot before clearing it (`state.rs:1785-1801`), and
`composer_ask_without_arguments_restores_exact_command_and_cursor` plus the
failure, Trust-redirect, and background-blocked tests retain focused coverage
(`state.rs:10974-11045`). No new defect was found in those paths.

## Checks

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib --locked` — passed, 499 passed and 0 failed. It emitted the recorded 72 `glass-browser` dead-code warnings.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_ --locked` — passed, 24 tests.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib pi_ --locked` — passed, 30 tests.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib pasted --locked` — passed, 5 tests.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check --manifest-path Cargo.toml -p glass-dev --lib --bins --locked` — passed with no new `glass-dev` warnings.
- `cargo fmt --all -- --check` and `git diff --check 9a2277b4^ 9a2277b4` — passed.
- `python3 scripts/check-documentation-depth.py` — passed (93 current guides routed/audited; 19 substantive contracts).
- `python3 scripts/check-release-documentation.py --require-previous-version` — passed (1,566 Markdown documents; 0 current-claim failures).
- `python3 scripts/check-tui-shortcuts.py` — passed (15 implementation keys; 63 documentation markers).

Non-blocking note: the task record's review-03 follow-up cites 1,565 Markdown
documents where this run scans 1,566. The gate result is unchanged (0
current-claim failures); the count differs only because the review document
itself adds a scanned file.
---
id: issue60-p1-tui-pi-draft-routing-001
scope: glass-dev/tui-pi-slash-composer-routing
status: in-progress
depends-on: [issue60-p1-tui-command-routing-001]
---

# Issue #60 P1 TUI-1: route Pi slash commands without losing composer drafts

## Objective

Resolve TUI-1 by making typed and pasted Pi slash commands share one dispatch
path while preserving the active Agent composer draft and focus across modal
open and close.

## Context

- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `docs/architecture/development-tui.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), TUI-1
- [Source report #41](https://github.com/wanazhar/glass/issues/41), composer slash routing
- [Source report #44](https://github.com/wanazhar/glass/issues/44), modal/composer transition
- [Source report #48](https://github.com/wanazhar/glass/issues/48), Pi slash modal
- [Completed F3 task](issue60-p1-tui-command-routing-001.md)

## Contract

- When the Agent composer is focused, typing `/` opens the Pi slash modal even
  when the draft is non-empty. Opening the modal preserves the complete draft
  and cursor. Esc closes it and restores composer focus with that exact draft
  and cursor. Command submission that closes the modal also restores composer
  focus; a pending mutation confirmation temporarily owns input, and denying
  it returns to the composer with the draft intact. Opening `/` outside the
  composer does not open the composer on close.
- While the composer is focused, a paste whose first non-whitespace character
  is `/` is treated as the modal's complete slash input, including multiline
  arguments. The existing composer draft remains unchanged. Other pasted text
  is inserted normally. Pasting while the slash modal is already open inserts
  into its command input. Typed and pasted slash input share a visible 4 KiB
  limit; overflow is not silently accepted.
- Typed modal input, leading-slash paste, and a slash command submitted from an
  existing composer draft use the same slash resolver and dispatch path.
  Native Pi commands retain their command and argument semantics.
- `/ask PROMPT` uses Ask mode and submits `PROMPT` whether entered by typing
  into the modal or pasted as `/ask PROMPT`; it does not submit a different
  string or erase the pre-existing composer draft. `/ask` without arguments
  changes to Ask mode and returns to the composer with the draft intact. The
  corresponding `/plan` and `/agent` aliases follow the same rule. `/todo`
  opens the Tasks surface while retaining the composer text and cursor, with
  composer focus inactive so Tasks keeps keyboard input. `/stats` and
  `/sessions` retain their existing Glass workspace-tool routes instead of
  being sent to Pi; typed and pasted forms use the same routes. `/think LEVEL`
  retains its Glass thinking-level tool route, while native `/thinking` stays
  a Pi slash command.
- Preserve the completed F3 command routing contract and keep Glass `:`
  commands in the separate workspace palette.

## Path

- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `docs/architecture/development-tui.md`
- `docs/plan/README.md`
- `docs/plan/analysis/issue-60.md`
- `docs/plan/tasks/issue60-p1-tui-pi-draft-routing-001.md`

## Verification

- Cover plain and multiline composer draft/cursor preservation through typed
  modal open and Esc close.
- Submit `/new keep this draft` directly from the composer, deny it, and verify
  the exact command text/cursor returns. Also verify retryable slash input is
  kept on dispatch failure or while another job runs, and a Trust redirect
  stays on Trust with the draft.
- Submit composer-origin `/ask` and `/todo` without opening the modal
  separately; verify each route changes mode/surface while retaining exact
  command text/cursor. Keep modal-origin unrelated drafts covered too.
- Compare typed and pasted `/ask what is this`: both select Ask mode, submit
  the same prompt, and preserve the composer draft. Cover `/ask` without
  arguments, existing `/stats`, `/sessions`, and `/think LEVEL` routes, and a
  native Pi command.
- Verify leading-slash paste extraction, ordinary paste insertion, paste into
  an already-open modal, and a slash command originating in composer history
  or existing input. Cover focus restoration after a native command enters and
  exits mutation confirmation, plus typed/pasted input at the size boundary.
- Run the focused TUI tests, `cargo check -p glass-dev --lib --bins --locked`,
  formatting, diff checks, and the relevant documentation gates. Record
  unrelated documentation inventory drift separately.

## Validation evidence

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib pi_ --locked` — passed (30 tests; 0 failed). Covers modal draft restoration, native Pi mutation confirmation return, multiline modal paste, the 4 KiB input boundary, `/think` versus native `/thinking`, and existing Pi command behavior.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_ --locked` — passed (18 tests; 0 failed), including composer send, history, alias, and `/todo` preservation coverage.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib pasted --locked` — passed (5 tests; 0 failed), covering typed/pasted `/ask` parity, `/stats`, `/sessions`, `/todo`, `/think`, and typed/pasted input limits.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib slash_paste_preserves_multiline_arguments_and_ordinary_paste_stays_in_composer --locked` — passed (1 test).
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib typed_slash_key_routes_multiline_draft_through_modal_escape_and_enter --locked` — passed (1 test); exercises the same narrow key-routing helpers used by the TUI event loop.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check --manifest-path Cargo.toml -p glass-dev --lib --bins --locked` — passed. `glass-browser` reports 72 existing dead-code warnings; `glass-dev` has no new warnings.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- `python3 scripts/check-documentation-depth.py` — passed (93 current guides routed/audited; 19 substantive contracts).
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-issue60-tui-pi-draft-release-documentation.json` — passed (1,563 Markdown documents scanned; 0 current-claim failures).
- `python3 scripts/check-tui-shortcuts.py` — passed (15 implementation keys; 63 documentation markers).
- `python3 scripts/check-documentation-coverage.py --glass /home/ubuntu/work/glass/target/debug/glass --glass-browser /home/ubuntu/work/glass/target/debug/glass-browser` — reports unchanged MCP inventory drift: live Glass tools differ from `crates/glass-dev/tests/fixtures/client-conformance-v1.json`; `docs/mcp-schema-budget.md` omits `| Negotiated tools | 177 |` and `| Serialized `tools` array | 76,967 UTF-8 bytes |`. This task does not change MCP tools or their schema.

## Review 01 follow-up

Review 01 found that submitting a native Pi command from composer input cleared
the source text before confirmation; denial therefore returned an empty draft.
The follow-up retains the composer-origin text and byte cursor while the
confirmation is pending, restores them on denial or failed submission, and
consumes that snapshot after a successful immediate queue. A modal opened with
an unrelated composer draft has no origin snapshot and retains its prior
behavior. Ask-mode failure restores the command while keeping the Trust surface
active if execution is redirected there.

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_native_slash_denial_restores_exact_command_and_cursor --locked` — passed (1 test).
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_slash_submit_failure_keeps_exact_input_for_retry --locked` — passed (1 test).
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_native_slash_blocked_by_background_restores_exact_input --locked` — passed (1 test).
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_ask_trust_redirect_keeps_input_on_trust_surface --locked` — passed (1 test).
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib pi_slash_command_waits_for_mutation_confirmation --locked` — passed (1 test), retaining separate-modal/unrelated-draft behavior.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check --manifest-path Cargo.toml -p glass-dev --lib --bins --locked` — passed; `glass-browser` reports the existing 72 dead-code warnings, with no new `glass-dev` warnings.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- `python3 scripts/check-documentation-depth.py` — passed (93 current guides routed/audited; 19 substantive contracts).
- `python3 scripts/check-release-documentation.py --require-previous-version` — passed (1,564 Markdown documents; 0 current-claim failures).

## Review 02 follow-up

Fresh review found that composer-origin `/ask` with no arguments and `/todo`
discarded their saved slash input when changing mode/surface. Both routes now
restore the composer-origin text/cursor after the transition. `/todo` leaves
composer focus inactive while Tasks owns the keyboard. Modal-origin commands
still preserve an unrelated existing draft as before.

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_ask_without_arguments_restores_exact_command_and_cursor --locked` — passed (1 test).
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_todo_route_restores_exact_command_and_cursor_on_tasks --locked` — passed (1 test).
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib ask_without_arguments_returns_to_composer_and_native_command_close_restores_focus --locked` — passed (1 test), retaining modal-origin unrelated-draft behavior.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib pasted_todo_opens_tasks_without_changing_composer_draft_or_cursor --locked` — passed (1 test), retaining modal-origin `/todo` behavior.

## Review 03 follow-up

Review 03 found that modal-origin or pasted `/todo` input kept the Composer
overlay active because there was no composer-origin snapshot to restore. The
Tasks route now clears composer focus unconditionally after closing and
restoring, while retaining the existing text/cursor for both modal-origin and
direct composer-origin commands. Both regressions require Tasks to own input
and check the retained buffer.

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_todo_route_restores_exact_command_and_cursor_on_tasks --locked` — passed (1 test).
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib pasted_todo_opens_tasks_without_changing_composer_draft_or_cursor --locked` — passed (1 test), including `active_overlay != Composer`.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib composer_ask_without_arguments_restores_exact_command_and_cursor --locked` — passed (1 test), preserving `/ask` composer focus behavior.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check --manifest-path Cargo.toml -p glass-dev --lib --bins --locked` — passed; `glass-browser` reports 72 existing dead-code warnings and `glass-dev` has no new warnings.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- `python3 scripts/check-documentation-depth.py`, `python3 scripts/check-release-documentation.py --require-previous-version`, and `python3 scripts/check-tui-shortcuts.py` — passed (93 guides/19 contracts; 1,565 Markdown documents with 0 current-claim failures; 15 implementation keys/63 documentation markers).

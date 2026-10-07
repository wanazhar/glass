# Independent review 03: TUI-1 Pi draft routing

Reviewed implementation commit `918bd069d5e85ba3f237ec257d6ec3b1469b2278` in a fresh worktree. The implementation branch currently has report-only commit `ed8d7ac4` above this code commit; this review targets the exact code commit requested.

## Verdict: BLOCKED

### P2 — Blocking: modal-origin `/todo` leaves Composer owning keyboard input

The `/todo` route closes the Pi modal, changes the surface to Tasks, then calls `restore_pi_composer_origin` (`crates/glass-dev/src/tui/state.rs:1803-1808`). For slash commands opened from the composer by typing `/` or pasting `/todo`, `open_pi_command_palette` records `pi_command_return_to_composer` (`state.rs:1577-1587`), but `pi_command_composer_origin` is `None`: the composer draft was left in place rather than consumed as a direct slash submission. The paste entry follows this path at `crates/glass-dev/src/tui/mod.rs:1406-1410`. The restore helper returns immediately when there is no origin (`state.rs:1606-1609`), leaving `composer_mode` true after `close_pi_command_palette` restored it.

That state makes `active_overlay` select Composer (`crates/glass-dev/src/tui/overlay.rs:60-64`). The event loop dispatches Composer input at `crates/glass-dev/src/tui/mod.rs:695-702` before the Tasks navigation/activation arms at `mod.rs:994-1010`. Consequently, after opening `/todo` from an existing composer draft, Down/Up and Space/Enter are routed to the composer instead of Tasks; the Tasks surface cannot receive normal keyboard navigation or activation.

The task contract requires `/todo` to retain the composer text and cursor **with composer focus inactive so Tasks keeps keyboard input** (`docs/plan/tasks/issue60-p1-tui-pi-draft-routing-001.md:50-52`); the Development TUI contract says the same (`docs/architecture/development-tui.md:356-358`). The new direct-composer test verifies inactive focus, but the existing `pasted_todo_opens_tasks_without_changing_composer_draft_or_cursor` test still asserts `composer_mode` is true (`state.rs:11340-11354`), which encodes the conflicting behavior and misses the focus failure. Reproduce by opening the Agent composer with a retained draft, pasting `/todo`, and pressing Enter; the surface changes to Tasks while the Composer overlay remains active. The Tasks route needs to deactivate composer focus for modal-origin routes too, while preserving the draft text and cursor.

## Re-review of prior findings

The review-02 draft-loss finding is fixed for direct composer-origin `/ask` and `/todo`: both routes restore exact text/cursor, and `/todo` disables Composer focus. The prior native mutation-denial fix remains: `deny_confirmation` restores the composer origin (`state.rs:4746-4757`), with direct denial covered by `composer_native_slash_denial_restores_exact_command_and_cursor` (`state.rs:10942-10970`). Submit failure, Trust redirect, and direct-origin background-blocked paths also retain focused regression coverage (`state.rs:10974-11045`). No additional blocker was found in those paths.

## Checks

- `git diff --check 918bd069^ 918bd069` — passed.
- The task record reports the focused denial/failure/Trust and direct `/ask`/`/todo` tests passing. I did not rerun Cargo; source and event-routing inspection establish the modal-origin focus defect, and the existing pasted `/todo` test asserts the incorrect focus state.

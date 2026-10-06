# Issue #60 P1 TUI command-routing review

## Scope reviewed

Reviewed the task contract and complete implementation diff from base `5c005790` through `2206a21a`, plus the visual-result remediation in `ac7aeb19`. The review worktree contains the same remediation patch as cherry-picked commit `85760a5a` (identical stable patch ID).

Checked typed-command versus selected-action resolution, shared menu/palette availability and dispatch, pointer activation, live-browser request reconciliation, agent-watch activation, and asynchronous ANSI/Herdr/Kitty result and failure handling.

## Findings

No P1, P2, or P3 findings. No blocking or non-blocking issues remain in the reviewed scope.

Relevant implementation evidence:

- `crates/glass-dev/src/tui/command.rs:31-62` preserves typed commands and arguments while resolving partial action prefixes. `crates/glass-dev/src/tui/state.rs:1377-1395,1684-1695,1743-1857` sends menu and palette selections through the same availability and dispatch path; `crates/glass-dev/src/tui/pointer.rs:565-615` covers pointer activation.
- `crates/glass-dev/src/tui/mod.rs:1378-1385,1234` ignores queued ANSI results after live view is disabled. Herdr failure/stop transitions disable and discard the worker at `mod.rs:347-355,1388-1417`, and the regression cases at `mod.rs:1691-1756` verify persistent semantic-only state and rejection of restart against the dead worker.
- The shared typed/palette/menu behavior and runtime reconciliation are covered by `state.rs:10827-10880` and `mod.rs:1763-1855`; failure regressions cover ANSI and Herdr result handling.

## Verification

Focused commands run in the review worktree with the shared target directory:

- `cargo test -p glass-dev --lib --locked stale_ansi_screenshot_is_ignored_after_live_view_stops` — passed (1 test).
- `cargo test -p glass-dev --lib --locked herdr_worker` — passed (2 tests).
- `cargo test -p glass-dev --lib --locked live_view` — passed (4 tests).
- `cargo test -p glass-dev --lib --locked palette_resolution_preserves_typed_routes_and_prefills_partial_actions` — passed (1 test).
- `cargo test -p glass-dev --lib --locked clicking_live_browser_action_queues_the_shared_menu_request` — passed (1 test).
- `git diff --check 2206a21a..HEAD` — passed.

The task record also reports passing package checks, formatting, and documentation gates. I did not repeat those broader checks for this focused review.

## Conclusion

**PASS.** Typed commands, selected palette actions, menu actions, and pointer activation follow the documented shared path. Live-browser state remains aligned after renderer failures, and stale ANSI success results no longer overwrite the semantic-only state after stop.

---
id: issue60-p1-tui-command-routing-001
scope: glass-dev/tui-command-routing
status: complete
depends-on: [issue60-p1-tui-overlay-priority-001]
---

# Issue #60 P1: unify TUI command and action routing

## Objective

Resolve F3 by routing typed palette commands, selected palette actions, and
command-center menu actions through one action resolver and one result/status
path. Keep live-browser state aligned with the visual runtime regardless of
whether the request comes from typed text, keyboard selection, or a pointer
click.

## Context

- `crates/glass-dev/src/tui/command.rs`
- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `crates/glass-dev/src/tui/pointer.rs`
- `docs/architecture/development-tui.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), finding F3
- [Source report #42](https://github.com/wanazhar/glass/issues/42)

## Contract

- Palette submission has one explicit resolver for typed commands versus
  filtered `SurfaceAction` choices. Complete typed commands and their
  arguments retain command semantics; partial input continues to filter and
  select actions or open the same argument prefill.
- Selecting the same `SurfaceAction` from the palette or menu uses the same
  resolver, availability check, argument prefill, command execution, and
  result/status handling. Shortcut actions retain their documented key
  behavior.
- `browser view` is a supported typed command and uses the same live-view
  request path as the menu and palette action. The TUI applies a visual request
  once, reports unavailable renderers consistently, and keeps the advertised
  browser presentation state synchronized with the runtime. Agent-watch
  activation and asynchronous screenshot failures also reconcile through that
  runtime path. Discard queued ANSI frames after live mode stops. A terminated
  Herdr worker moves this runtime to Semantic-only mode and future live
  requests report its persistent failure reason.
- Pointer menu selection uses the same action path as keyboard menu selection;
  it cannot claim live view started without applying the visual-runtime
  request.
- Keep palette roots and the More-surface command horizon in the scope of
  their separate F33 finding.

## Verification

- Verify typed commands, fuzzy palette selections, and selected actions resolve
  according to the documented precedence, preserving command arguments and
  existing `open`, `search`, `doctor`, and other direct command behavior.
- Invoke a shared action from both the menu and palette and verify the same
  availability decision, argument prefill, and resulting command/status.
- Exercise `browser view` as typed input, a palette action, a keyboard menu
  action, and a pointer menu action. Verify each changes runtime state through
  one request path, including agent-watch activation and renderer-unavailable
  and screenshot-failure paths. Verify workspace presentation and runtime live
  flags agree after each result. Verify queued ANSI success results are ignored
  after stop and terminated Herdr workers cannot be restarted as live.
- Run focused TUI command, state, pointer, and visual-runtime tests, package
  checks, formatting, and independent review.

## Validation evidence

- `cargo test -p glass-dev --lib palette_resolution_preserves_typed_routes_and_prefills_partial_actions --locked` — passed.
- `cargo test -p glass-dev --lib typed_palette_selected_action_and_menu_share_live_view_dispatch --locked` — passed; typed input, filtered action, and keyboard menu dispatch request the same live-view state/status.
- `cargo test -p glass-dev --lib menu_and_selected_palette_action_share_placeholder_prefill --locked` — passed.
- `cargo test -p glass-dev --lib menu_and_selected_palette_action_share_unavailable_reason --locked` — passed; both entry points expose the same unavailable action state.
- `cargo test -p glass-dev --lib palette_filters_actions_and_prefills_selected_arguments --locked` — passed.
- `cargo test -p glass-dev --lib clicking_live_browser_action_queues_the_shared_menu_request --locked` — passed; pointer click dispatches through the menu action path.
- `cargo test -p glass-dev --lib agent_watch_requests_and_reconciles_live_presentation --locked` — passed.
- `cargo test -p glass-dev --lib visual_request_reconciliation_keeps_runtime_state_and_presentation_aligned --locked` — passed for unavailable-renderer handling.
- `cargo test -p glass-dev --lib ansi_screenshot_failure_clears_runtime_and_workspace_live_state --locked` — passed.
- `cargo test -p glass-dev --lib stale_ansi_screenshot_is_ignored_after_live_view_stops --locked` — passed with a valid decoded PNG fixture; a queued success after stop preserves SemanticOnly presentation and the stopped status.
- `cargo test -p glass-dev --lib failed_herdr_worker_stays_disabled_after_stopped_and_rejects_restart --locked` — passed; the terminal failure survives the subsequent Stopped event, and a later enable request remains SemanticOnly with the failure reason.
- `cargo test -p glass-dev --lib stopped_herdr_worker_rejects_restart_with_a_persistent_reason --locked` — passed; a normal terminal Stopped event also prevents retry against the dead worker.
- `cargo test -p glass-dev --lib typed_open_in_palette_runs_instead_of_fuzzy_action --locked` — passed.
- `cargo check -p glass-dev --lib --bins --locked` — passed; `glass-browser` reported its existing dead-code warnings.
- `cargo build -p glass-dev --bin glass --locked` and `cargo build -p glass-browser --bin glass-browser --locked` — passed for documentation inventory prerequisites.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation.json` — passed.
- `python3 scripts/check-documentation-depth.py` — passed.
- `python3 scripts/check-tui-shortcuts.py` — passed.
- `python3 scripts/check-documentation-coverage.py` — reports that the live MCP
  tool set differs from its conformance fixture and
  `docs/mcp-schema-budget.md` omits the live measurements (`177` tools and
  `76,967` serialized bytes). Those MCP files are unchanged by this task.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Independent review passed: [review report](../reviews/issue60-p1-tui-command-routing-001-01.md).

Finding F3 is complete.

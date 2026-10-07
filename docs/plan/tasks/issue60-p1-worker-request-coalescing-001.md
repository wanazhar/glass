---
id: issue60-p1-worker-request-coalescing-001
scope: glass-dev/tui-selection-previews
status: in-progress
depends-on: [issue60-p1-tui-pointer-geometry-001]
---

# Issue #60 P1: coalesce TUI selection previews

## Objective

Resolve F7 by bounding Git diff, process log, and debugger detail refreshes to
one active selection preview and one replaceable latest request. Ensure only a
result still owned by the current visible selection can update TUI state.

## Context

- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `crates/glass-dev/src/tui/snapshot.rs`
- `docs/harness-architecture.md`
- `docs/architecture/development-tui.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), finding F7
- [Source report #42](https://github.com/wanazhar/glass/issues/42), section 7

## Contract

- Git diff, process log, and debugger selection previews use at most one active
  worker request and one replaceable pending request; rapid selection changes
  retain the latest desired target for eventual loading.
- Each preview result is tied to its job ID and captured selection key. A
  result may update the UI only while that owner and the visible selection
  still match; stale results cannot overwrite a newer selection or status.
  Debug thread and stack previews also require the pane that requested them.
  Scopes and variables require the selected frame plus the captured
  Debug/Frames view or the Code source path opened for that frame. Code-side
  results may update debugger data without changing Code's status or surface.
  Absolute DAP paths are opened only after resolving inside the current
  workspace; ownership retains the original frame path and normalized Code
  path separately.
- Selecting an untracked Git entry immediately presents its no-diff
  placeholder and invalidates prior tracked-diff results.
- Explicit user operations outside these read-only selection previews retain
  their existing submission, confirmation, and completion semantics.
- The source report's claim that selection moves enqueue an unbounded number of
  requests does not reproduce on this baseline: `background_action_running()`
  already blocks another selection submit while a tool job is running. The
  confirmed failures are dropped latest selections and stale result ownership,
  including an untracked Git placeholder being overwritten by a previous diff.

## Path

- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `docs/harness-architecture.md`
- `docs/architecture/development-tui.md`
- `docs/plan/README.md`
- `docs/plan/analysis/issue-60.md`
- `docs/plan/tasks/issue60-p1-worker-request-coalescing-001.md`
- `docs/plan/reviews/issue60-p1-worker-request-coalescing-001-01.md`

## Verification

- Add focused tests for bursts across Git/process/debug selection, latest
  pending ownership, stale-result rejection, and Git untracked placeholder
  preservation. Also test stale thread/stack results after a pane change and
  scopes/variables finishing in Code after a source jump without replacing
  Code status.
- Run focused TUI tests, `cargo check -p glass-dev --lib --bins --locked`,
  formatting, diff checks, and relevant documentation checks.
- Record unrelated documentation-coverage drift separately from F7 evidence.

## Validation evidence

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path /home/ubuntu/work/glass-issue60-p1-worker-request-coalescing-001/Cargo.toml -p glass-dev selection_preview_bursts_keep_one_latest_git_process_or_debug_target --lib --locked` — passed (1 test). The regression covers Git → process → debugger request bursts, replacement of the latest pending target, an unrelated result ID being ignored, stale Git result rejection, and eventual submission of the latest debugger request. Returning to the active Git selection also cancels the obsolete pending request and restores its loading status.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path /home/ubuntu/work/glass-issue60-p1-worker-request-coalescing-001/Cargo.toml -p glass-dev stale_git_diff_cannot_replace_untracked_placeholder --lib --locked` — passed (1 test); a tracked-diff result leaves the selected untracked-file placeholder and status intact.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check --manifest-path /home/ubuntu/work/glass-issue60-p1-worker-request-coalescing-001/Cargo.toml -p glass-dev --lib --bins --locked` — passed. It reports 72 existing dead-code warnings in `glass-browser`; no new `glass-dev` warnings.
- `cargo fmt --manifest-path /home/ubuntu/work/glass-issue60-p1-worker-request-coalescing-001/Cargo.toml --all -- --check` and `git -C /home/ubuntu/work/glass-issue60-p1-worker-request-coalescing-001 diff --check` — passed.
- `python3 /home/ubuntu/work/glass-issue60-p1-worker-request-coalescing-001/scripts/check-documentation-depth.py` — passed; 93 current guides routed/audited and 19 substantive contracts.
- `python3 /home/ubuntu/work/glass-issue60-p1-worker-request-coalescing-001/scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-issue60-f7-release-documentation.json` — passed; 0 current-claim failures.
- `python3 /home/ubuntu/work/glass-issue60-p1-worker-request-coalescing-001/scripts/check-tui-shortcuts.py` — passed; 15 implementation keys and 63 documentation markers.
- `python3 /home/ubuntu/work/glass-issue60-p1-worker-request-coalescing-001/scripts/check-documentation-coverage.py --glass /home/ubuntu/work/glass/target/debug/glass --glass-browser /home/ubuntu/work/glass/target/debug/glass-browser` — reports existing MCP inventory drift: live tools differ from `crates/glass-dev/tests/fixtures/client-conformance-v1.json`, and the schema budget lacks `| Negotiated tools | 177 |` and ``| Serialized `tools` array | 76,967 UTF-8 bytes |``. This selection-preview change does not touch MCP tools or schema.

## Review follow-up

- Review 01 (`docs/plan/reviews/issue60-p1-worker-request-coalescing-001-01.md`)
  is blocked on pane ownership for thread/stack results and scopes being
  discarded after a source jump. The follow-up adds pane/view ownership,
  result-status suppression in Code, and focused regressions
  `stale_debug_thread_and_stack_previews_do_not_take_over_new_pane` and
  `debug_scopes_and_variables_finish_after_source_jump_without_replacing_code_status`.
- The Code-source regression uses an absolute DAP path under the temporary
  project, verifies the focused path normalizes to `src/main.rs`, and checks
  the original DAP path remains part of preview ownership.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked debug_` — passed (8 tests), including both new debugger-pane and source-jump regressions.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked stale_git_diff_cannot_replace_untracked_placeholder` — passed (1 test).
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-dev --lib --bins --locked` — passed; 72 existing dead-code warnings in `glass-browser`, no new `glass-dev` warnings.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- `python3 scripts/check-documentation-depth.py` — passed; 93 current guides routed/audited and 19 substantive contracts.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-issue60-f7-followup-release-documentation.json` — passed; 0 current-claim failures.
- `python3 scripts/check-tui-shortcuts.py` — passed; 15 implementation keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py --glass /home/ubuntu/work/glass/target/debug/glass --glass-browser /home/ubuntu/work/glass/target/debug/glass-browser` — the known MCP fixture/schema drift remains: 177 live tools and 76,967 serialized bytes are absent from the fixture/budget. This F7 change does not touch MCP.
- Task status remains `in-progress` pending independent re-review.

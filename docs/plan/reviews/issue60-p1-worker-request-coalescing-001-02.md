# Issue #60 P1 TUI selection-preview review 02

## Scope reviewed

Reviewed the full F7 task implementation from `f68428ae` through
`47830a38147cf89bbf78370d647ddf2f7caa5994`, with focused re-review of the
follow-up over review 01. Read the task contract, its validation record, the
Development TUI and harness-worker contracts, and the changed implementation
and regressions in `crates/glass-dev/src/tui/state.rs`. Rechecked the shared
project path resolver in `crates/glass-dev/src/development/project.rs`. No
implementation files were changed.

## Findings

### P2 · non-blocking — Add a rejection regression for out-of-workspace DAP paths

The new regression uses an absolute DAP path inside a temporary workspace and
asserts that it opens as `src/main.rs`, while retaining the original DAP path
and normalized Code path in the preview key
(`state.rs:13225-13269`). It does not exercise an absolute path outside the
workspace or an absolute symlink that resolves outside it.

The current implementation fails closed: `debug_frame_source_path()`
canonicalizes both the workspace root and absolute source, then requires the
source to strip the canonical root prefix (`state.rs:5481-5493`). Relative
paths are subsequently handled by `ProjectWorkspace::resolve_path()`, which
rejects parent traversal and checks canonical parents and existing targets
against the project root (`project.rs:1693-1727`). This is a coverage
recommendation, not a current escape: add a focused rejection test for an
outside absolute path, ideally including a symlink escape where supported.

## Rechecked blockers

- **Pane ownership is fixed.** Thread and stack keys record Sessions and
  Threads respectively, and validation requires the same current pane before
  a result can apply (`state.rs:2146-2165`). The regression injects late thread
  and stack results after pane changes and verifies they preserve the user's
  pane and status (`state.rs:13155-13221`).
- **Scopes and variables survive the intentional source jump and remain
  view-bound.** A frame-detail key retains frame ID and original DAP path, plus
  the normalized source path when captured from Code
  (`state.rs:2167-2205, 5350-5434`). The validator accepts Debug only in
  Frames, or Code only while focused on the captured source path
  (`state.rs:2096-2117`). The jump canonicalizes and confines absolute DAP
  paths before opening them (`state.rs:5437-5493`). The regression verifies
  both scopes and variables load after the jump without changing Code's
  surface or status (`state.rs:13225-13320`).

## F7 contract and validation

The shared preview lane still bounds selection previews to one active job and
one replaceable pending request (`state.rs:2088-2094, 2210-2277`). The full
task includes regressions for mixed Git/process/debug bursts, latest pending
ownership, stale job IDs, and the untracked Git placeholder
(`state.rs:12935-13153`). Explicit user operations remain on their existing
submission and confirmation path. Code-side scope/variable success and error
statuses are suppressed while debugger data can still be updated
(`state.rs:2200-2243, 4160-4176, 4303-4317, 4473-4479`).

I did not rerun Cargo because the implementation task records the focused
debugger command as passing and no additional build was needed for this source
review. The task records
`CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked debug_`
passing 8 tests, including both blocker regressions, plus the focused untracked
Git regression, package check, formatting, and documentation checks. I
independently ran `git diff HEAD^ HEAD --check`; it passed.

## Conclusion

**PASS.** Both review 01 blockers are closed, the F7 latest-wins ownership
contract remains satisfied, and absolute DAP paths are confined to the
workspace. The remaining P2 item is a non-blocking regression-test
recommendation; no current functional blocker was found.

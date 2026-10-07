# Issue #60 P1 TUI selection-preview review

## Scope reviewed

Reviewed commit `1da631673450ff0b60928b1dd29ac3fcd5d5ea5f` against parent
`f68428aebb096f454a8e6a0720097b20e2b9a421`. Read the F7 task contract and
reviewed the implementation and regression tests in `state.rs`, `mod.rs`, and
`snapshot.rs`, along with the Development TUI and harness-worker contracts.
The review covered Git/process/debug preview submission, latest-pending
replacement, result IDs and selection keys, untracked Git placeholder handling,
worker refresh/result dispatch, and preservation of explicit tool operations.
No implementation files were changed.

## Findings

### P1 · blocking — Debug scopes are discarded after jumping to source

`jump_selected_debug_frame()` opens the frame path, then sets
`debug_scopes_requested` (`crates/glass-dev/src/tui/state.rs:5343-5375`). For a
trusted workspace, `open_path()` calls `open_selected_file_for_edit()` and
switches the active surface to Code (`state.rs:3472-3490`). The event loop then
routes the flag to `queue_debug_scopes()` (`crates/glass-dev/src/tui/mod.rs:1102-1104`).
That method captures `surface: self.surface` in the key (`state.rs:5276-5296`),
but `selection_preview_key_is_current()` requires `DevSurface::Debug` for the
`DebugScopes` target (`state.rs:2081-2084, 2120-2128`).
`flush_pending_selection_preview()` removes the request and rejects it when
that validator fails (`state.rs:2196-2200`).

Repro: with a trusted workspace, select a debugger frame with a source path and
press Enter to jump to it. The TUI switches to Code and queues the scopes
refresh, but validation drops it before `SnapshotWorker::submit_tool`; scopes
and the subsequent variables preview never load on this normal route. Before
this change, the same event-loop flag submitted the scopes request without the
new surface check. Preserve the debugger request's owner across the intentional
Code navigation, or otherwise allow the still-current frame's preview to load.

### P1 · blocking — Late debugger previews can override a newer pane and status

The debugger selection keys check session/thread/frame identity but omit
`debug_pane` (`state.rs:2105-2128`). `cycle_debug_pane()` changes the pane and
status without invalidating a pending preview (`state.rs:5025-5035`). A matching
`glass.debug.threads` result then forces the pane back to Threads and replaces
status (`state.rs:4214-4227`); a matching `glass.debug.stack` result similarly
forces Frames and replaces status (`state.rs:4228-4237`).

Repro: on Debug/Sessions, select session A so its thread preview is queued.
Before it completes, cycle to Frames. The selected session remains A, so the
captured key still validates; when the result arrives, the handler switches
back to Threads and overwrites the newer pane status. The same occurs for a
stack request if the user cycles away while its selected session and thread
remain unchanged. This violates the task requirement that a result update state
only while the owner and visible selection still match. Include the relevant
pane or a user-navigation generation in the ownership check, and cover both
result paths with a delayed-result regression.

## Positive findings

The shared preview lane keeps one replaceable pending request, validates its
selection before submission, and flushes it after a worker result. Git,
process, and debugger keys check their captured item identities; stale results
with the wrong job ID are ignored. Selecting an untracked Git entry clears the
pending preview, installs the local placeholder, and causes an active tracked
diff result to fail its current-selection check. The task's burst and untracked
tests cover those cases. The task's correction to the source report's
unbounded-submit claim is supported by the parent implementation: the Git,
process, and debugger preview submitters already returned while
`background_action_running()` was true. The concrete latest-selection and
ownership failures are the relevant remaining F7 gaps.

## Verification

- I did not rerun Cargo commands because the task requested avoiding a second
  build while the shared target might be in use. The task record reports the
  focused burst regression and untracked-placeholder regression passing, plus
  the package check and formatting/documentation checks (`docs/plan/tasks/issue60-p1-worker-request-coalescing-001.md:64-73`).
- Findings above are based on direct source-path tracing and the stated
  repro sequences; the two missing debugger cases are not covered by the
  current focused regressions.

## Conclusion

**BLOCKED.** The normal debugger source-jump route drops its scopes preview,
and late thread/stack previews can take control of a pane the user has already
left. Both need fixes and regressions before F7 passes review.

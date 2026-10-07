# Issue #60 P1 browser-crash recovery review 01

## Scope reviewed

Reviewed implementation commit `f324d79617822db1784fabaf7155fe3c8aebe0cc`
against its parent `9200be2037aeb7142f8a29c6a6584cead6b84f3c`. Read the F19
task contract, the Development TUI recovery/overlay contract, and the Issue #60
delivery mapping. Audited crash snapshots, qualifying tool failures, overlay
priority/input/render ownership, dismissal, queued navigation, and stale visual
events. No implementation files were changed.

## Findings

### P2 · blocking — A delayed Herdr event can erase the pending recovery status

The new architecture contract says the status line identifies browser recovery
while its offer is pending (`docs/architecture/development-tui.md:155-161`).
After a crash, `offer_browser_recovery` installs the offer and recovery status
(`crates/glass-dev/src/tui/state.rs:9799-9804`), then
`BrowserWorkspaceController::disconnected` changes the connection and
invalidates semantic entities but leaves `browser_revision` and
`frame_revision` untouched (`crates/glass-browser/src/browser_workspace/mod.rs:471-479,674-679`).

With the App surface and Composer active, a delayed `HerdrEvent::Connected` can
then pass `handle_herdr_event`'s rendered-area and matching-frame checks:
Composer is intentionally non-occluding (`crates/glass-dev/src/tui/overlay.rs:26-33`),
and the event handler does not require the browser connection to remain
Connected (`crates/glass-dev/src/tui/mod.rs:1461-1474`). It replaces the
recovery status with `Live view ready · Herdr pane graphics`. A delayed Herdr
failure also publishes `Live view unavailable…` without checking for a pending
recovery offer (`crates/glass-dev/src/tui/mod.rs:1476-1481,442-458`). The offer
remains pending, but the documented status disappears; after Composer closes,
the recovery overlay appears alongside a status unrelated to the recovery.

Suggested regression: start with App + Composer, a pending live Herdr frame
whose `frame_revision` matches `browser_revision`, apply a `BrowserHealth::Crashed`
snapshot, then deliver delayed Herdr Connected and Failed events. Assert the
recovery offer and recovery status remain authoritative. Clear the frame
revision on disconnect or gate stale visual status updates while recovery is
pending.

## Resolved paths and review notes

- **Surface, picker, and navigation preservation — source path is correct.**
  The crash path no longer assigns `DevSurface::App` and saves/reapplies the
  recovery status around snapshot projection (`state.rs:8154-8170,8327-8331`).
  The failure path no longer closes the target picker or changes the surface
  (`state.rs:9778-9804`); the controller disconnect only invalidates browser
  semantics. Neither offer path clears `pending_browser_navigation`.
  `dismiss_browser_recovery` clears that navigation explicitly and leaves the
  selected surface unchanged (`state.rs:9806-9812`).
- **Overlay priority — pass.** `BrowserRecovery` is last in the shared resolver
  (`overlay.rs:36-70`), which the input loop, pointer router, renderer, and
  terminal overlay mask consume. The resolver regression sets every other
  overlay active in turn (`overlay.rs:138-230`). Snapshot coverage retains
  Trust, Code/full-screen editor, Help, Quit, and Terminal (`state.rs:11052-11102`);
  the failure regression retains Help and the target picker, including its
  request flag and query (`state.rs:11104-11142`).
- **Qualifying failure routing — pass for the async tool-result path.** The
  failure matcher lowercases the error and returns whether it installed an
  offer (`state.rs:9778-9797`); `apply_tool_job_result` suppresses the generic
  error status when recovery was offered (`state.rs:4470-4477`). The focused
  regression exercises `glass.browser.start` with `DevTools connection refused`.
  I also traced the synchronous command wrapper: `command::execute` can call
  `note_browser_failure`, after which its callers set a command error status
  (`command.rs:816-824`; `state.rs:1811-1820,5602-5612`). The current
  `run_tool` command route only queues tool work (`command.rs:3036-3089`), so
  actual browser execution failures use the async handler above; I found no
  current synchronous qualifying tool-failure path.
- **Queued-navigation test gap — P3, non-blocking.** The implementation retains
  pending navigation when an offer is installed and clears it on dismissal, as
  specified, but the new F19 regressions do not seed `pending_browser_navigation`
  to assert either transition. Add a focused assertion when convenient.

## Verification

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked browser_recovery` — passed: 3 passed, 0 failed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked crashed_snapshot_preserves_surfaces_and_defers_recovery_behind_active_ui` — passed: 1 passed, 0 failed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked browser_tool_failure_preserves_picker_and_overlay_state` — passed: 1 passed, 0 failed.
- `git diff --check f324d796^ f324d796` — passed.

## Conclusion

**BLOCKED.** Surface preservation and overlay precedence pass, but the stale
Herdr event path can replace the recovery status while the offer is pending.

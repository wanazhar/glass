# Issue #60 P1 F9 visual-state review

## Scope reviewed

Reviewed the complete F9 implementation from base `cb772190ad46651a598bbaf16421cfd6ac068931` through `f71332358fe94ad22383b26df5ee6e120e9d9fab`, including the lifecycle implementation and its two follow-up fixes. Read the task contract and source finding #42 section 9. Checked rendered-area gating, Composer visibility, hidden in-flight results, Kitty clearing, ANSI/Herdr freshness and status, and explicit live-view off/on behavior. No implementation files were changed.

## Findings

No P1, P2, or P3 findings.

- `render::browser_visual_area` derives the App panel from `app_surface_geometry` and the shared `visual_pixel_area`; `draw_ansi_pane` uses that same pixel inset (`render.rs:377-397, 4425-4435`). The App renderer uses the same geometry for its visual and Workflow panels (`render.rs:2859-2877, 2938-2948`). Composer remains non-occluding (`overlay.rs:31-38`). The regression checks the visual bounds against the rendered Workflow-aware geometry (`render.rs:5051-5065`).
- Screenshot scheduling requires a connected browser, live runtime, and rendered visual area, and sizes the request from that area (`mod.rs:370-376, 1244-1252`). Visibility reconciliation clears hidden ANSI state and frame revision while preserving the live toggle; result ownership rejects a capture hidden in flight even after the pane returns (`mod.rs:378-439`). The regression covers pause, resume, stale-result rejection, and acceptance of a fresh frame (`mod.rs:1770-1887`).
- Kitty graphics are cleared when the tracked pane changes, including when the rendered area becomes absent (`mod.rs:253-267, 497-510, 1158-1170`). The accepted-result path recomputes the same shared visual area before emission (`mod.rs:1273-1324`).
- A delayed Herdr `Connected` event only reports readiness when a present frame revision matches the current browser revision (`mod.rs:1465-1478`); hiding clears the frame revision. The regression keeps the resuming state while such an event arrives (`mod.rs:1891-1916`). Explicit live-view off clears cached frame state and marks the pending result hidden before re-enable (`mod.rs:387-395`), covered by `visual_manual_toggle_off_clears_cached_frame_and_rejects_in_flight_result` (`mod.rs:1919-1968`).
- The paused/resuming panel reason takes precedence over a selected backend’s generic active placeholder (`render.rs:2731-2764, 5072-5090`). Unrelated global status messages remain intact as documented in the task and architecture contract.

## Verification

- `git diff cb772190ad46651a598bbaf16421cfd6ac068931 HEAD --check` — passed.
- The task record reports `cargo test -p glass-dev --lib --locked visual_` passed (12 tests), the package check and formatting passed. I did not run Cargo, per the review instruction.

## Conclusion

**PASS.** The reviewed implementation ties capture and display to the same rendered pixel bounds, pauses and resumes without accepting hidden frames, clears Kitty graphics on hide, and preserves freshness across Herdr and explicit toggle transitions.

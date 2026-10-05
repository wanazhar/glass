# Native engine Slice 850 review

- **Revision reviewed:** Slice 850 worktree based on `3152b5fe`
- **Task:** `native-engine-browser-850`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. No blocking contract mismatch was found.

## Review notes

- The loopback regression uses a real process-backed ServiceWorker timer turn.
  It registers the ServiceWorker only after the page's visible cookie setter,
  then waits for and dispatches the content process's async-effect
  notification. It verifies `/page`, `/sw.js`, `/timer-first`, `/timer-upload`,
  and `/timer-second` order, including the upload method, content type, and
  exact body (`crates/glass-browser/src/browser/native_engine/engine.rs`,
  `native_content_process_service_worker_timer_fetch_uses_parent_cookie_authority`).
- The initial focused run failed the newly added first-request assertion
  because the fixture had registered a zero-delay timer before setting the
  page cookie. Reordering the fixture makes the owner-tagged page write happen
  before registration; the final run passes without modifying runtime cookie
  code.
- The parent broker validates the exact context, frame, generation, and
  document URL for the request owner, rejects cookie writes with a different
  owner, and applies accepted writes to the parent loader before selecting
  request cookies (`content_process.rs:6668-6760, 7635-7666, 7717-7757`).
  Response `Set-Cookie`, matching, and persistence remain parent-side. The
  child receives only the bounded visible `document.cookie` projection; the
  test confirms HttpOnly values remain absent from that projection.
- Stream chunks are gathered through the existing bounded demand-driven IPC
  and the request is collected before HTTP dispatch. The task and architecture
  docs make no socket-level upload streaming or backpressure claim.
- The scoped test check, content-worker build, focused timer tests, formatting,
  and documentation audits passed locally. There is no remote-CI,
  cross-platform, WPT, or release result in this review.

## Conclusion

**Pass (direct self-review; not an independent review).**

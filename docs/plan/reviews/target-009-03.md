# Final re-review: target-009 deterministic element targeting

Reviewed branch through `039ffbb` (`fix: make pointer dispatch cancellation
safe`).

Conclusion: **blocked**. CSS secrecy, ancestor visibility, remote-object reuse,
and performance are corrected. One narrow but real press-cancellation race
remains, plus cleanup on bounded-query error paths.

## Findings

### P1 — blocking: the release guard is armed after awaiting the press response

For each event, `dispatch_pointer_events` first awaits
`dispatch_mouse_event(...)` and only then constructs `PressedButtonGuard` when
the event was `mousePressed` (`src/browser/session.rs:888-902`). Chrome applies
the input before returning the CDP response. MCP cancellation can therefore
drop the operation while that response is pending, after the page has received
`mousedown` but before `pressed` contains any guard. No best-effort release is
issued in that window, so the original stuck-button failure remains possible.
A CDP response timeout/error after Chrome accepted the press has the same
effect.

The new real-Chrome test waits until `mousedown` is observable and then drops
the click future (`tests/browser_smoke.rs:924-950`), which usually occurs after
the fast local CDP response has already resumed the future and armed the guard.
It proves cancellation during the dwell, not cancellation during the press
request itself.

Arm the release guard immediately before sending `mousePressed`; if the press
definitively fails, a redundant best-effort release is safer than leaving an
accepted-but-unacknowledged press stuck. Disarm only after an acknowledged
release. Add a deterministic delayed-CDP-response test that applies the press,
holds its response, cancels the caller, and observes a release.

### P2 — non-blocking: bounded-query remote objects leak on intermediate errors

The successful `bounded_element_query` path now releases each child object
after `DOM.requestNode` and releases the array at the end
(`src/browser/cdp.rs:449-479`). However, `Runtime.getProperties(...).await?`
returns early without releasing the already-created array, and an error during
one `DOM.requestNode` releases only that child before `?` exits, skipping the
array and remaining child handles (`src/browser/cdp.rs:424-479`). This is an
error-path Chrome-side resource leak. Use an object group/drop cleanup or an
explicit finally-style result wrapper so all acquired remote handles are
released on success, cancellation, and intermediate error.

## Resolved target-009-02 findings

- Press-boundary revalidation and the ordinary cancellation-during-dwell path
  now issue a release through `PressedButtonGuard`; the blocker above is only
  the pre-guard press-response window (`src/browser/session.rs:874-918`,
  `1184-1226`).
- Ambiguous CSS candidates use strategy-only labels such as `css match 1`, and
  the sentinel assertion confirms the raw selector is absent
  (`src/browser/session.rs:1046-1054`, `1282-1306`;
  `tests/browser_smoke.rs:822-840`).
- Exact visible-text lookup now checks `checkVisibility`, ancestor opacity/
  rendering, and clipping intersections. Fixtures cover opacity-zero and
  fully clipped ancestors without making the unique visible phrase ambiguous
  (`src/browser/session.rs:1324-1351`, `tests/fixtures/basic.html:23-27`).
- The action path resolves one remote node object, reuses it for initial and
  press-boundary checks, and releases it through a guard
  (`src/browser/session.rs:803-831`, `1184-1205`). This preserves same-node
  identity while reducing CDP round trips; no correctness or secret regression
  was found in that reuse.
- Reference click p95 is now 18.02 ms versus the recorded 17.15 ms baseline,
  about a 5% difference rather than the prior 85% regression. p50 is 16.89 ms,
  workflow RSS is 6,950,912 bytes, compact context is 15,835 bytes, and the
  binary is 4,530,136 bytes; all recorded release gates pass
  (`docs/plan/tasks/target-009.md:73-82`). This is not a material regression on
  the published sample, though larger release comparisons remain appropriate.

## Focused verification

I reused the branch's recorded full browser, cancellation, scorecard, and
benchmark runs. Independent inexpensive checks passed:

- `cargo test browser::session::tests --lib` — 16 passed;
- `cargo test browser::cdp::tests --lib` — 8 passed;
- `cargo test mcp::server::tests --lib` — 14 passed;
- `git diff --check 237d542 039ffbb`.

No implementation changes were made during this review.

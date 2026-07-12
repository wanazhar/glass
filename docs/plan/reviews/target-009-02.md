# Re-review: target-009 deterministic element targeting

Reviewed branch through `237d542` (`fix: preserve targeting correctness through
dispatch`).

Conclusion: **blocked**. Press-boundary revalidation, typed MCP failures,
bounded remote match projection, and the main text-locator cases are improved.
Cancellation-safe release, selector secrecy, complete visibility semantics,
and the recorded fast-path regression still prevent completion.

## Remaining findings

### P1 — blocking: cancellation can still leave the mouse button pressed

The architecture now claims that once pressed Glass “always releases the
button” so cancellation cannot leave Chrome in a stuck state
(`docs/architecture/browser.md:78-83`). The implementation still dispatches
each generated event in an ordinary cancellable async loop and sleeps after a
human `mousePressed` (`src/browser/session.rs:870-884`). MCP cancellation drops
the entire browser operation future through `tokio::select!`; a cancellation
during that sleep, while awaiting `mouseReleased`, or between double-click
press/release pairs prevents the release from being sent. A CDP error on the
release path has the same result.

Press-boundary revalidation correctly closes the hover-reflow window before
the first button event, but it does not establish the documented release
guarantee. Once a press succeeds, shield/complete a bounded release sequence
before honoring cancellation, with an explicit best-effort cleanup path on
transport error. Add an MCP real-browser test that cancels during the human
press delay and proves a matching release reaches Chrome.

### P1 — blocking: CSS ambiguity errors leak raw selectors through MCP

MCP safely serializes `TargetError`, but CSS resolution passes
`format!("css={selector}")` into `dom_nodes_resolution`
(`src/browser/session.rs:1017-1025`). Ambiguous candidate labels are then
derived from that label (`src/browser/session.rs:1230-1238`). Because MCP emits
those candidates verbatim (`src/mcp/server.rs:474-487`), a private selector in
an ambiguous request is echoed in the error response. This contradicts the
new documentation that raw targets/selectors are never echoed
(`docs/mcp.md:71-77`) and regresses the MCP secret boundary established in
`mcp-008`.

Use strategy-only candidate labels such as `CSS match 1`, never the raw
selector or text query. Add sentinel tests for ambiguous CSS as well as
not-found, stale, and actionability paths. Successful `ActionOutcome` labels
also need a documented secrecy policy because unique CSS/text labels currently
contain the caller input (`src/browser/session.rs:1221-1227`).

### P2 — blocking: visible-text matching ignores invisible ancestors

The new text expression is exact and whitespace-normalized and correctly
handles the committed hidden, nested, duplicate, and selector-like fixtures.
However, visibility is tested only on each walked element's own computed
`display`, `visibility`, and `opacity` plus its rectangle
(`src/browser/session.rs:1258-1265`). Opacity is not inherited. A visible-sized
span inside an `opacity: 0` ancestor has computed opacity `1`, retains a
non-zero rectangle and `innerText`, and is therefore returned as a text match
even though it is not rendered visibly. Similar ancestor clipping can produce
the same mismatch.

This violates the documented “exact rendered `innerText` only” contract
(`docs/architecture/browser.md:53-56`). Validate the promoted interactive
owner and its ancestor rendering chain (or use a bounded hit/visibility test)
before counting a candidate. Add invisible-ancestor and clipped-owner cases,
including ambiguity where one duplicate is invisible.

### P2 — blocking: the measured reference-click p95 materially regressed

The task verification requires reference-path round trips and allocations not
to regress materially (`docs/plan/tasks/target-009.md:45-47`). The updated
completion records click p95 increasing from 17.39 ms to 32.26 ms—about 85%—on
the same 100-iteration/20-click methodology
(`docs/plan/tasks/target-009.md:75-82`). Calling it a follow-up optimization
target and noting that release-wide memory gates pass does not satisfy this
task-specific non-regression check. The safety revalidation is necessary, but
the milestone must either optimize/batch the verification path, ratify an
explicit revised action-latency budget with comparative evidence, or move the
non-regression requirement to a named blocking follow-up before remaining
`done`.

### P3 — non-blocking: bounded query child handles are not explicitly released

`bounded_element_query` releases the remote array but not the element object
handles returned by `Runtime.getProperties`
(`src/browser/cdp.rs:424-467`). It is unclear that releasing the array releases
those separately exposed remote handles. Repeated CSS/text resolution may
therefore retain Chrome-side handles until context teardown. Assign an object
group to evaluation and release the group, or explicitly release child handles;
add a repeated-query resource test.

## Resolved findings from review 01

- Both fast and human pointer travel now revalidate the same node and center
  after movement and before `mousePressed`; hover-triggered geometry changes
  fail before button events (`src/browser/session.rs:831-869`,
  `tests/browser_smoke.rs:861-883`).
- Target failures are typed as ambiguous, not-found, stale, or not-actionable,
  with bounded reasons/candidates, and MCP serializes only this structured type
  (`src/browser/session.rs:177-228`, `932-969`; `src/mcp/server.rs:474-487`).
  The selector leak above is the remaining secret-safety defect.
- CSS and text expressions retain at most eight remote elements; CDP returns
  only the bounded property set/node IDs, and AX name/role match collection
  stops after nine (`src/browser/session.rs:972-1031`, `1244-1276`;
  `src/browser/cdp.rs:396-468`).
- The browser smoke now resets its pointer ledger before rejected actions and
  proves no `mousedown`/`mouseup` occurred, while permitting harmless movement
  used to detect hover reflow (`tests/browser_smoke.rs:820-883`).
- Text matching now has defined exact normalized semantics and committed tests
  for ordinary hidden content, nested ownership, duplicate text, and a
  CSS-looking literal (`tests/browser_smoke.rs:788-819`). The ancestor case
  above remains.
- Allocation evidence is now honestly described as peak workflow RSS rather
  than allocator instrumentation, with the measured delta disclosed
  (`docs/plan/tasks/target-009.md:79-82`).

## Focused verification

I reused the branch's recorded full browser, scorecard, and benchmark runs.
Independent inexpensive checks passed:

- `cargo test browser::session::tests --lib` — 16 passed;
- `cargo test browser::cdp::tests --lib` — 8 passed;
- `cargo test mcp::server::tests --lib` — 14 passed;
- `git diff --check ee61ba6 237d542`.

No implementation changes were made during this re-review.

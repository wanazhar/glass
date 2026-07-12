# Re-review: topology-011 browser targets and frames

Reviewed commit: `1aa99d5` (`fix: harden browser topology routing`).

Prior review: `docs/plan/reviews/topology-011-01.md`.

Conclusion: **blocked**. The patch resolves the retained-ID bounds finding and
adds useful route scoping, flattened auto-attach plumbing, explicit CLI route
options, identity on structured page/action/wait results, and substantially
broader real-browser coverage. The operation route is still not immutable
end-to-end, nested-frame pointer translation is still incorrect, detach
lifecycle handling is incomplete, and the required frontend/resource evidence
is not yet present. Keeping the task `in-progress` is correct.

## Blocking findings

### P1 — operation routing is scoped, but operation identity and topology reads remain mutable

`OPERATION_ROUTE` snapshots the CDP session/context/frame and the focused CDP
unit test proves that two direct `send` calls retain their original session.
That resolves the narrow request-routing race. It does not provide an immutable
per-operation route to `BrowserSession`: `route_identity` reads the mutable
topology registry, while `target_viewport_point` calls `list_frames`, which also
reads the currently selected target/session. A selection racing an observation
or action can therefore keep its CDP calls on the old task-local route but label
the result with the new target/frame, or use the new target's frame tree while
computing an old-route action.

Selection rollback is also incomplete. `select_target` commits the new registry
and global CDP route before `Target.setAutoAttach`, `list_frames`, and
`select_frame`; an error in any of those later steps returns without restoring
the old route/registry. The individual `set_active_context`,
`set_active_frame_context`, and `set_active_frame` methods can still publish
partial route mutations outside one atomic route replacement.

Expose one captured route handle to the complete `BrowserSession` operation,
derive both requests and returned identity from it, and make target/frame
selection transactional through all fallible setup. Add a session-level race
test that checks returned identity and frame lookup, plus injected-failure tests
for every post-commit selection step. The current CDP-only test is insufficient
for the original finding.

### P1 — nested and flattened-frame pointer routing is not correct or proven

`target_viewport_point` adds only the selected frame owner's offset. For a frame
nested inside another frame it must accumulate every ancestor owner's offset;
the current implementation cannot produce the top-level point for depth two or
greater. The integration fixture called `nested` is actually a direct
`about:srcdoc` child beside the cross-site iframe, so it does not exercise the
nested-frame requirement or expose this bug.

Flattened auto-attach and a frame-to-session map are now present, and the smoke
test clicks a cross-site frame. However, the test does not assert that Chrome
actually supplied a distinct OOPIF session. The coordinate lookup is issued
through the selected operation session, even though the frame owner belongs to
the parent session; there is no explicit parent-session/owner-chain model to
make that behavior deterministic for nested OOPIFs.

Model each frame's parent frame and owning session, translate through the full
ancestor chain using the session that owns each frame element, and add forced
distinct-session assertions. Cover offset same-origin depth-two click/type,
nested OOPIF click, and a top-page decoy proving the wrong element did not
receive input.

### P1 — flattened detach and frame lifecycle state can remain stale

The new event handler preserves an unrelated selected frame for ordinary
sibling frame events, which fixes the previous indiscriminate deselection.
But page frame events are accepted only when their event session equals
`active_session_id`. Once an OOPIF is selected, root-session lifecycle events
are ignored. On `Target.detachedFromTarget`, the handler finds an arbitrary
frame mapped to that session and clears selection only if that one frame is the
selected frame. Multiple frames discovered in the same child session make a
selected descendant dependent on `HashMap` iteration order; stale mappings and
selection can survive the session loss.

There are still no focused sibling-navigation, nested-subtree detach, active
OOPIF-session detach, or session-loss recovery tests. The real `Page.crash`
case covers target loss, not the child-session detach contract from the prior
review.

Track lifecycle by target session independently of the selected frame session,
remove every frame owned by a detached session, clear selection when it is in
that removed subtree, and test each required detach/sibling case.

### P2 — required frontend and resource verification remains incomplete

The new real-browser smoke test is valuable, but its CLI and MCP processes are
given target/frame IDs discovered by the in-process library and perform only
evaluation. It does not demonstrate either frontend's required discovery
through frame **action** chain. MCP does not call topology tools in the chain,
and the CLI does not discover the IDs it later consumes.

The task records only approximate RSS endpoints and p50 cached/fresh
observation latency. It omits the task's reproducible p95 latency, context
payload bytes, CDP request counts/round trips, and binary-size delta, and does
not point to a retained report artifact. The test also lacks a genuinely nested
frame and explicit child-session detach scenario. These are release-gate
requirements, not optional polish.

Add real CLI and MCP list/select/frame-click chains and retain a benchmark
report conforming to `benchmarks/report-schema.json`, including p50/p95,
payload, request, RSS/peak, and binary measurements.

## Resolved prior findings

- Retained topology IDs are now consistently limited: opener IDs are validated,
  session/frame IDs are validated before retention, and event-summary IDs use
  the 256-byte UTF-8-safe bound.
- Explicit `--target-id` and `--frame-id` globals make a routed one-shot attached
  CLI invocation usable; the remaining CLI issue is verification of the full
  discovery-to-action workflow.
- `PageInfo`, `PageContext`, `ActionOutcome`, and `WaitOutcome` now expose
  target/frame identity. Raw-value primitives such as `evaluate` still cannot
  report identity, and the identity source remains mutable as described above.
- Popup discovery remains non-selecting, and active target close/crash clears
  routing rather than guessing a replacement.

The prior non-blocking typed-topology-error/MCP-sanitization finding remains
open and should be handled before the public topology contract is considered
agent-friendly, but it is not the reason for this blocked verdict.

## Focused verification

Independent inexpensive checks passed:

- `cargo test browser::cdp::tests --lib` — 9 passed;
- `cargo test browser::session::tests --lib` — 19 passed;
- `cargo test cli::args::tests --lib` — 9 passed;
- `cargo test mcp::server::tests --lib` — 14 passed;
- `git diff --check 34e072a..1aa99d5` — clean.

The real-Chrome smoke test was reviewed as retained implementation evidence but
was not rerun during this focused re-review. Its assertions do not cover the
blocking gaps above.

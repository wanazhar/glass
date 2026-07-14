---
id: observe-019
scope: event-driven fresh accessibility observation
status: closed-infeasible
depends-on: [compare-018]
---

# Make fresh accessibility observation event-driven

## Objective

Bring fresh compact-observation p95 below 5 ms without weakening the complete,
route-bound accessibility projection or its mutation-race detection.

## Context

The reviewed release candidate measured 8.95-9.11 ms p95 for fresh compact
observation. Chrome's 4.1-5.1 ms `Accessibility.getFullAXTree` call dominates
that path. Reusing an isolated execution world improved p95 only to 8.28 ms
and was reverted. Depth caps, threshold relaxation, and unbracketed concurrent
reads are prohibited.

## Phase 0: feasibility gate

No mirror implementation or architecture claim may ship until both gates pass:

1. Establish from Chromium source both that a specific routed command response
   orders all accessibility updates caused by earlier renderer mutations and
   that every projection-relevant mutation emits sufficient updates to rebuild
   the same tree. Completeness evidence must cover new previously unrequested
   nodes, removal, reparenting, ignored-state and visibility changes, computed
   names/roles/values, navigation, and frame changes. The public CDP
   description of `getRootAXNode` guarantees neither ordering nor completeness,
   so it is not currently an accepted fence. Adversarial real-Chrome traces
   cover the same cases even if source evidence is found.
2. Benchmark the proposed barrier plus immutable mirror copy, reachability
   pruning, and compact projection for three 50-iteration release runs. The
   combined steady-state fresh observation must plausibly remain <=5 ms p95
   before concurrency and lifecycle machinery is built.

If ordering cannot be proven, close this task without an event mirror and keep
the complete-tree path. A fast result that can silently lag the page is not an
optimization.

## Phase 0 result

Closed without implementation. Inspection of the exact pinned Chromium
`150.0.7871.115` source found three incompatible semantics in
`InspectorAccessibilityAgent`:

Source: [Chromium 150 InspectorAccessibilityAgent](https://chromium.googlesource.com/chromium/src/+/refs/tags/150.0.7871.115/third_party/blink/renderer/modules/accessibility/inspector_accessibility_agent.cc)

- `getFullAXTree` walks and returns the tree but does not add those returned
  node IDs to `nodes_requested_`.
- dirty notifications are retained only for IDs in `nodes_requested_`;
  `getRootAXNode` adds only the root, while registering descendants requires
  explicit child requests.
- `ProcessPendingDirtyNodes` throttles update publication to 250 ms, with the
  next scheduling check occurring after that interval.

Consequently a full-tree seed does not receive complete descendant updates,
and making the requested-node set complete would still leave immediate
post-action observations either stale for roughly 250 ms or falling back to
the existing complete-tree call. That violates both the correctness contract
and the <=5 ms useful fresh-observation goal. The candidate contract below is
retained only as rejected design evidence. Reopen only if the pinned Chromium
semantics change or a different supported protocol supplies a complete,
ordered snapshot barrier.

## Candidate contract after Phase 0

Glass may maintain one accessibility mirror for the exact selected
target/session/frame route. The mirror has these states:

```text
absent -> seeding -> ready
   ^         |         |
   +---------+---------+
     route change, navigation, event loss,
     malformed update, or budget overflow
```

- Enabling `Accessibility` and one complete tree read seed the mirror. Seeding
  is single-flight; concurrent observations do not start duplicate reads. The
  identity includes target, CDP session, frame, document loader/generation, and
  Accessibility-enable generation.
- The event consumer assigns a monotonic sequence before seeding starts. It
  buffers same-generation events while `getFullAXTree` is in flight, installs
  the seed only if route and document generations are unchanged, replays
  buffered updates in sequence, and then changes to ready atomically. Event
  loss, `loadComplete`, navigation, detach, crash, or generation change during
  seeding aborts that seed.
- `Accessibility.nodesUpdated` replaces nodes by stable AX node id.
  `Accessibility.loadComplete` invalidates the old tree and requires a new
  complete seed. Parent `childIds` define reachability; unreachable nodes are
  pruned before publication.
- Every event is matched to the exact CDP session and seed generation. The
  selected-frame seed root defines membership: after applying a whole event
  batch to a temporary map, only nodes reachable from that root may enter the
  mirror. A new node is accepted only when its parent is already a member and
  the updated parent links it, or when the same batch establishes that chain.
  Unknown and unrelated nodes remain quarantined. A broken chain invalidates
  the generation. Target, session, frame, root, or document changes discard the
  mirror; state is never transferred.
- Broadcast lag, a missing root or child, duplicate structural identity,
  malformed payload, navigation, or a node/byte budget overflow marks the
  mirror unusable. The current observation performs one bounded full-tree
  rebuild rather than returning partial accessibility data.
- Before a ready mirror is read, Glass uses only the ordering barrier proven in
  Phase 0. Barrier error or timeout invalidates the mirror and triggers the
  bounded complete-tree path.
- The existing page-state samples continue to bracket the accessibility
  snapshot. A DOM/page revision or in-page mutation change retries once and
  then publishes the existing explicit `MutationRace` incompleteness marker.
- Mirror ownership is bounded to 4,096 nodes and 2 MiB of accounted retained
  node ids, structural ids, roles, names, values, properties, and map capacity.
  It never retains screenshots, full DOM, page text, or response history.
  Overflow invalidates the mirror. If its bounded complete-tree fallback also
  fails, the observation returns that error rather than empty or partial AX.
- Domain disable/re-enable, target detach/crash, incomplete structural updates,
  and a referenced missing child invalidate the generation and require a seed.
  The implementation does not claim it can detect an event Chromium omitted;
  that is why source-level completeness evidence is a Phase-0 prerequisite.
- `snapshot` and explicit deep accessibility paths continue to request a fresh
  complete tree. Only compact observation may consume the mirror.

## Verification

- Unit tests cover single-flight seed, ordered node replacement, reachability
  pruning, route isolation, navigation invalidation, event loss, malformed
  updates, and node/byte budget fallback.
- A real-Chrome oracle test first reaches a defined event-quiescent point, then
  compares mirrored compact output with a complete
  tree after attribute, text, role, visibility, insertion, removal, reorder,
  shadow-root, iframe, navigation, and rapid-mutation cases. Any false match or
  missing actionable reference is blocking. Comparison canonicalizes node
  order and normalizes observation revision/reference fields; collection that
  mutates between the two reads is discarded and retried.
- A real-Chrome event-loss/reseed test proves the observation returns complete
  rebuilt data, never a partial mirror.
- Three release-mode runs of at least 50 fresh observations must each report
  p95 <=5 ms on the ratified fixture. Cached observation remains <=0.1 ms p95,
  idle Glass RSS remains <=8 MiB, and compact output remains <=32 KiB.
- Keep the implementation only after full tests, clippy with warnings denied,
  the native release matrix, and independent adversarial review pass.

## Commit

`perf: mirror accessibility updates`

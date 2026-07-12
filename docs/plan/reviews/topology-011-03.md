# Final re-review: topology-011 browser targets and frames

Reviewed commit: `6b3dce9` (`fix: close topology routing gaps`).

Prior review: `docs/plan/reviews/topology-011-02.md`.

Conclusion: **blocked** on release evidence only. The remaining implementation
blockers from the prior review are resolved, but the required benchmark artifact
and two measured values are still absent.

## Remaining blocker

### P2 — resource evidence does not record CDP count, artifact size, or a retained report

`examples/benchmark.rs` now emits `cdp_request_count_after_workload` alongside
`glass_binary_size_bytes`, and the task records RSS, p50/p95 latency, and compact
payload bytes. However, `docs/plan/tasks/topology-011.md` states only that the
benchmark “reports” CDP request count and binary artifact size; it records no
actual value for either field and links to no retained JSON report. Therefore
the final one-page resource gate cannot be independently checked against the
schema or compared with the prior release baseline.

Retain the benchmark JSON artifact and record its CDP request count and binary
size in the task. No implementation change is required for this finding.

## Resolved blockers

- Operation identity now comes from the same task-local route snapshot as CDP
  requests, including target and frame IDs. The route race test verifies both
  request routing and returned operation identity.
- `select_target` performs event setup, flattened auto-attach, and main-frame
  discovery before publishing the new topology and route; preparation failures
  detach the candidate session without replacing the old selection.
- Real-browser coverage now includes a true depth-two grandchild frame click
  and asserts that the cross-site frame is mapped as `out_of_process` before
  evaluating and clicking it.
- Child-session detach removes every mapping owned by that session, clears an
  active detached frame route, and frame lifecycle events from both the root
  target session and retained child sessions are accepted.
- The CLI discovers target and frame IDs through its own `targets` and `frames`
  commands before routed evaluation. A single interactive MCP session calls
  `listTargets`, `selectTarget`, `listFrames`, `selectFrame`, and `evaluate` in
  order.
- The task now records cached/fresh p50 and p95 plus compact payload bytes, and
  the benchmark output includes CDP-count and binary-size fields.

## Focused verification

This gate used direct inspection of the focused diff from `4ef9d8b` through
`6b3dce9`. No broad build or real-browser rerun was performed, per review scope.

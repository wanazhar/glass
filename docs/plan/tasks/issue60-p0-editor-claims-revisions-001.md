---
id: issue60-p0-editor-claims-revisions-001
scope: glass-dev/editor-claims-and-buffer-revisions
status: pending
depends-on: [issue60-p0-tui-data-preservation-001]
---

# Issue #60 P0: make editor claims and checkpoint restores atomic

## Objective

Resolve findings F86 and F89. Reject overlapping write claims even when they
belong to the same actor, publish both claim and release transitions, and make
selection replacement and checkpoint restore update buffer revision, undo,
cache, dirty, and original-content state as one mutation.

## Context

- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), findings F86/F89
- [Source report #50](https://github.com/wanazhar/glass/issues/50)

## Contract

- A write claim conflicts with any overlapping active write claim, including
  claims from the same actor; releasing a claim emits a visible state change.
- Selection replacement and checkpoint restore either complete every related
  buffer-state update or leave the original state intact.
- Successful edits advance revision, append exactly one undo state, invalidate
  derived caches, update dirty/original-hash metadata, and publish consistent
  events.
- A restored checkpoint is clean only when its contents match the recorded
  original content and hash.

## Path

- `crates/glass-dev/src/development/graph.rs`
- `crates/glass-dev/src/development/project.rs`
- Editor buffer and checkpoint state in `crates/glass-dev/src/`
- Relevant collaboration/editor contract documentation

## Verification

- Add overlapping-claim tests for same and different actors, including claim
  and release event publication.
- Add selection-replacement and checkpoint-restore tests for revision, undo,
  cache invalidation, dirty/original-hash state, and rollback on failure.
- Verify event, snapshot, and persisted project views agree after each change.

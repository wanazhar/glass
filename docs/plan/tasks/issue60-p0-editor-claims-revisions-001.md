---
id: issue60-p0-editor-claims-revisions-001
scope: glass-dev/editor-claims-and-buffer-revisions
status: complete
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
  partial overlaps from the same actor. An exact same-actor range re-claim
  updates that existing claim slot; releasing a claim emits a visible state
  change.
- Selection replacement and checkpoint restore either complete every related
  buffer-state update or leave the original state intact.
- Successful edits advance revision, append exactly one undo state, invalidate
  derived caches, update dirty/original-hash metadata, and publish consistent
  events.
- A restored checkpoint is clean only when its contents match the recorded
  original content and hash.

## Path

- `crates/glass-dev/src/development/collaboration.rs`
- `crates/glass-dev/src/development/events.rs`
- `crates/glass-dev/src/development/project.rs`
- Editor buffer and checkpoint state in `crates/glass-dev/src/`
- Relevant collaboration/editor contract documentation

## Verification

- Add overlapping-claim tests for same and different actors, including claim
  and release event publication.
- Add selection-replacement and checkpoint-restore tests for revision, undo,
  cache invalidation, dirty/original-hash state, and rollback on failure.
- Verify event, snapshot, and persisted project views agree after each change.

## Verification evidence

- `cargo test -p glass-dev --lib --locked development::` — 66 passed.
- `cargo test -p glass-dev --lib --locked` — 423 passed.
- `cargo test -p glass-dev --lib --locked resident_native_dialog_controller_resolves_suspended_navigation_out_of_band` — passed after building the required native content worker.
- `cargo check -p glass-dev --lib --bins --locked` and `cargo build -p glass-dev --bins --locked` — passed.
- `cargo fmt --all -- --check`, `git diff --check`, release-documentation truth, documentation depth, and TUI shortcut checks — passed.
- `scripts/check-documentation-coverage.py` still reports the development MCP tool fixture and schema-budget measurements are stale (live list: 177 tools, 76,967 bytes). This is outside F86/F89 and remains in the Issue #60 MCP findings.
- Independent review passed with no remaining blockers.

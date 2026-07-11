---
id: action-003
scope: browser actions
status: done
depends-on: [perf-001, lifecycle-002]
---

# Stable references and low-cost input actions

## Objective

Make compact-snapshot controls safe to reuse and improve click behavior without adding implicit slow waits.

## Context

- `docs/architecture/browser.md`
- `docs/plan/analysis/performance-overhaul.md`

## Path

- `src/browser/cdp.rs`
- `src/browser/session.rs`
- `src/browser/mouse.rs`
- `src/cli/args.rs`
- `src/cli/runner.rs`
- `tests/`
- `docs/architecture/browser.md`

## Requirements

- Publish revisioned backend-node element references and reject stale references.
- Scroll a resolved target into view only when required before pointer dispatch.
- Preserve existing fast and human pointer semantics.
- Expose explicit double-click and drag primitives only if they can share the same low-cost action contract.
- Return structured, serializable action outcomes for frontend use; do not add implicit navigation/network-idle waits.

## Verification

- Unit tests for stale reference parsing/rejection and action result serialization.
- Local browser fixture coverage for off-screen interaction and fast/human motion.
- Full fmt, Clippy, and all-target test suite.

## Commit

`feat: add revisioned browser actions`

## Completion

- Compact and full accessibility controls publish revisioned backend-node
  references in the `r<revision>:b<backend-node-id>` form.
- Stale references fail before a full accessibility-tree lookup can select a
  different control.
- Click, double-click, type, and scroll return serializable action outcomes
  containing the resulting page revision.
- Chrome receives `DOM.scrollIntoViewIfNeeded` before pointer input; fast and
  human pointer modes retain their existing event/timing behavior.
- Drag remains internal to the mouse engine until it has a reliable
  target/viewport contract.

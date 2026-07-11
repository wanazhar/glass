---
id: target-009
scope: deterministic element targeting
status: pending
depends-on: [quality-007]
---

# Resolve unique targets and verify action points

## Objective

Replace first-match convenience targeting with typed unique, ambiguous, and
not-found results, and verify pointer hit targets immediately before dispatch.

## Context

- `docs/architecture/automation.md`
- `docs/architecture/browser.md`

## Path

- `src/browser/dom.rs`
- `src/browser/session.rs`
- `src/browser/cdp.rs`
- `src/cli/`
- `src/mcp/`
- `tests/`
- `docs/architecture/browser.md`

## Requirements

- Make reference, accessible name, role+name, text, CSS, and ordinal strategies
  explicit.
- Never choose the first substring or role-only match silently.
- Return bounded candidate summaries on ambiguity.
- Check attachment, visibility, enabled state, viewport, stable geometry, and
  `elementFromPoint` ownership before pointer input.
- Preserve the revisioned-reference fast path.

## Verification

- Adversarial duplicate-label, overlay, sticky-header, reflow, disabled, and
  detached-element tests.
- Zero wrong-target actions across the scorecard repetitions.
- Fast reference path round trips and allocations do not regress materially.

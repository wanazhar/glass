---
id: target-009
scope: deterministic element targeting
status: done
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

## Completion

- Added explicit reference, exact accessible-name, role+name, text, CSS, and
  one-based ordinal locator strategies. Bare references retain the fast path;
  bare strings are exact accessible names. Role-only and substring-name
  selection no longer choose the first control.
- Resolution now returns unique, not-found, or ambiguity internally. Ambiguity
  exposes at most eight UTF-8-safe 160-byte candidate summaries, and CSS/text
  searches inspect match counts instead of using the first DOM result.
- Pointer actions resolve the remote node, scroll it with nearest alignment,
  reject detachment, hidden/disabled state, active animation, unstable or
  off-viewport geometry, and require center-point hit ownership before any
  pointer event is dispatched.
- CLI and MCP document the same locator forms. MCP's legacy `selector`
  argument is explicitly converted to CSS rather than reinterpreted as an
  accessible name.
- Adversarial real-Chrome coverage exercises duplicate names/selectors,
  role-only input, overlays, sticky occlusion, active reflow, disabled state,
  and detachment during scrolling. The full seven-test browser suite passes.
- A three-iteration optimized scorecard recorded 18 successes, three known
  delayed-content failures, 12 unsupported outcomes, and zero wrong actions.
  All three duplicate-label repetitions selected `right-target`.
- The 100-iteration optimized benchmark measured the revision-reference fast
  click at 16.72 ms p50 and 17.39 ms p95 over two CDP round trips (20 click
  samples), versus the recorded 17.15 ms baseline p95. Compact context was
  10,764 bytes, Glass RSS ended at 6,352,896 bytes, and the stripped binary was
  4,464,528 bytes (no change from `mcp-008`). Allocator-level peak
  instrumentation is unavailable in the release profile; process RSS is the
  conservative host measurement.

## Commit

`feat: enforce deterministic element targeting`

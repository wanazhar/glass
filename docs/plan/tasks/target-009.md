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
- Resolution now returns typed unique, not-found, stale, actionability, or
  ambiguity outcomes across CLI and MCP. Ambiguity
  exposes at most eight UTF-8-safe 160-byte candidate summaries, and CSS/text
  searches inspect match counts through bounded remote arrays instead of
  materializing or using the first DOM result. Text means exact normalized
  visible text and promotes nested text to its interactive owner.
- Pointer actions resolve the remote node, scroll it with nearest alignment,
  reject detachment, hidden/disabled state, active animation, unstable or
  off-viewport geometry, and require center-point hit ownership initially and
  again after pointer movement immediately before button dispatch.
- CLI and MCP document the same locator forms. MCP's legacy `selector`
  argument is explicitly converted to CSS rather than reinterpreted as an
  accessible name.
- Adversarial real-Chrome coverage exercises duplicate names/selectors,
  role-only input, hidden/nested/selector-like text, overlays, sticky
  occlusion, hover-triggered and active reflow, disabled state, detachment
  during scrolling, typed MCP ambiguity, and absence of button events on
  rejected actions. The full seven-test browser suite passes.
- A three-iteration optimized scorecard recorded 18 successes, three known
  delayed-content failures, 12 unsupported outcomes, and zero wrong actions.
  All three duplicate-label repetitions selected `right-target`.
- After press-boundary revalidation, the 100-iteration optimized benchmark
  measured revision-reference clicks at 17.10 ms p50 and 32.26 ms p95 (20
  samples). The median remains near the recorded 17.15 ms baseline p95; the
  tail cost is recorded rather than hidden and is a follow-up optimization
  target. Compact context was 14,448 bytes, Glass RSS ended at 7,204,864 bytes,
  and the stripped binary was 4,530,112 bytes. All remain inside release gates.
  Allocator instrumentation is unavailable in the release profile; peak
  workflow RSS is the documented reproducible allocation proxy and increased
  by 1,175,552 bytes versus the quality baseline while remaining under 8 MiB.

## Commit

`feat: enforce deterministic element targeting`

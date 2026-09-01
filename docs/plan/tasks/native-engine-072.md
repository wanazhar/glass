---
id: native-engine-072
scope: glass-browser/native-engine/flex-align-content-space-around
status: active
depends-on: [native-engine-071]
---

# Native engine 072: bounded flex cross-line space-around

## Objective

Extend the completed 071 wrapped-row `align-content` owner with one additional
bounded value, `space-around`, while preserving the existing 064 through 071
line formation, visual order, fixed widths, margins, gap, justification,
cross-axis item alignment, direction, wrapping, shared artifact geometry, root
scrolling, and two-crate package boundary.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. This slice composes only with the completed
`native-engine-064` through `native-engine-071` flex-row, gap, justification,
visual-order, item-alignment, direction, wrapping, and bounded cross-line
alignment slices.

## Contract

The native CSS grammar adds the non-inherited keyword `space-around` to
`align-content`. The existing accepted values remain
`flex-start|center|flex-end|space-between`; the default remains `flex-start`.
`stretch`, `space-evenly`, `normal`, logical values, CSS-wide keywords,
safe/unsafe modifiers, and malformed values continue to produce the existing
bounded unsupported-value diagnostic and use the `flex-start` fallback.

For a block-level `display:flex` container that passes the existing eligible
direct-element gate and uses `flex-wrap:wrap`:

- line formation, line sizes, item sizing, per-line `align-items`, main-axis
  placement, and complete line artifact ownership remain unchanged;
- when explicit resolved content-box height has positive remainder after the
  formed line heights, `space-around` places each line at the center of one
  equal integer slot. The line-start offset for line `i` is
  `floor(remainder * (2*i + 1) / (2*line_count))`, with checked bounded
  arithmetic; this gives half a slot before the first line, one slot between
  neighboring lines, and half a slot after the last line under deterministic
  integer rounding;
- the offset formula distributes only positive free space, never creates a
  negative line coordinate, and leaves auto-height and undersized content boxes
  stacked as before;
- `space-around` applies to the complete line artifact ranges after
  `align-items`. Direct item boxes, descendants, text runs, display-list
  entries, viewport projection, point hit testing, root overflow, and capture
  therefore consume the same shifted coordinates;
- a single formed line uses half of the available positive remainder before
  the line, while `nowrap` ignores `align-content` and remains coordinate- and
  paint-equivalent to 071;
- semantic DOM traversal, source text, node references, accessibility order,
  and keyboard order remain document order. The value changes visual geometry
  only.

The property remains bounded cross-line placement, not general Flexbox or
browser conformance.

## Explicit exclusions

This slice does not add `space-evenly`, `stretch`, `place-content`,
cross-axis gaps, `row-gap`, `column-gap`, flex growth/shrink/basis, auto
margins, column directions, `wrap-reverse`, logical direction/RTL, intrinsic
or percentage sizing, nested scrolling, positioned or stacking layout,
keyboard or accessibility reordering, or browser Flexbox parity. It does not
change non-flex normal flow, source/semantic traversal, or the stable two-crate
package topology.

## Tradeoffs and risks

The formula uses integer document pixels and floors each line's ideal offset.
That makes the result reproducible and bounded, but it cannot represent
fractional half-slots or subpixel Flexbox distribution. Checked `u64`
intermediate arithmetic prevents a large free-space/line-index product from
wrapping before it is converted back to the engine's bounded `u32` geometry.
The default and all previously supported values retain their existing paths;
only the new parser value selects the additional offset formula.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: add the bounded
  `SpaceAround` computed value, cascade, inline declaration, and diagnostics;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: add the checked
  integer line-offset formula to the existing complete-artifact second pass;
- `crates/glass-browser/tests/native_engine.rs`: parser/cascade rejection and
  acceptance, single- and multi-line explicit-height geometry, auto/smaller
  heights, `nowrap` equivalence, descendants, paint, hit testing, overflow,
  and semantic-order coverage;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- [ ] CSS unit tests cover accepted `space-around`, defaulting, rejected
  values, non-inheritance, selector cascade, and inline precedence;
- [ ] integration tests cover the checked offset formula for one and multiple
  lines, explicit/auto/smaller heights, `nowrap` equivalence, descendants,
  root overflow, hit testing, paint, and semantic/source-order fallback;
- [ ] the full native integration suite, strict default/native Clippy,
  formatting, whitespace, and documentation validators pass;
- [ ] implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed;
- [ ] exact regenerable Cargo outputs are reclaimed after all validation
  without terminating long-lived Glass processes.

## Completion evidence

Implementation and validation evidence will be recorded here after the
documented slice is implemented. The branch is local-only until a separately
authorized push, so remote CI and release status must not be inferred.

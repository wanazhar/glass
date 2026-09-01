---
id: native-engine-069
scope: glass-browser/native-engine/flex-direction
status: in-progress
depends-on: [native-engine-068]
---

# Native engine 069: bounded flex direction

## Objective

Add a bounded, deterministic `flex-direction` property to the existing
eligible single-row flex layout. This slice adds physical reverse-row
placement while preserving the existing item filtering, fixed widths, margins,
`gap`, `justify-content`, `align-items`, shared artifact geometry, and
two-crate package boundary.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. The implementation composes with the completed
`native-engine-064` through `native-engine-068` flex-row, gap, justification,
visual-order, and cross-axis-alignment slices.

## Contract

The native CSS grammar accepts `flex-direction` only as the non-inherited
keywords `row` and `row-reverse`. The default used value is `row`, preserving
the pre-069 geometry. `column`, `column-reverse`, logical direction keywords,
CSS-wide keywords, and malformed values produce the existing bounded
unsupported-value diagnostic and use the default fallback.

For a block-level `display:flex` container that passes the existing eligible
single-row gate:

- `row` is coordinate- and paint-equivalent to the completed 068 behavior;
- visible direct element children retain the existing `(order, source_index)`
  visual sort, fixed widths, physical margins, one non-negative pixel `gap`,
  `justify-content` distribution, and `align-items` cross-axis placement;
- `row-reverse` lays the order-sorted visual sequence from the physical right
  edge toward the left, so the lowest used order is nearest the right edge and
  the highest used order is nearest the left edge; source/semantic order stays
  unchanged;
- in `row-reverse`, `flex-start` anchors the first order-sorted item at the
  physical right side, `flex-end` anchors the sequence at the physical left
  side, `center` uses the same integer half-free-space policy, and
  `space-between` distributes only positive free space across the existing
  gaps in visual order;
- physical left/right margins remain attached to their item boxes. The reverse
  walk consumes the right margin, item width, and left margin in that order;
- each complete item subtree is laid out and translated as one artifact range,
  so descendant boxes, text runs, display-list paint, viewport projection,
  root overflow, and point hit testing use the same reverse-row geometry;
- an overflowing reverse row is shifted as one bounded row by its measured
  excess so its leftmost item begins at the non-negative content origin. The
  row may extend to the right and remains reachable through the existing root
  horizontal-scroll path; no item receives a negative coordinate;
- hidden/`display:none` children remain absent and do not consume a sort, gap,
  justification, or reverse-row position; ineligible flex containers retain
  normal-flow fallback and ignore `flex-direction`;
- semantic DOM traversal, source text, node references, accessibility order,
  and keyboard order remain document order. Direction is visual placement, not
  a semantic or interaction reorder.

The property is visual layout state, not a general writing-direction or
Flexbox-conformance claim.

## Explicit exclusions

This slice does not add `column`/`column-reverse`, `flex-wrap`, multiple lines,
`align-content`, `place-content`, logical `direction`/`writing-mode`, RTL
text-direction semantics, cross-axis auto margins, flex growth/shrink/basis,
percentage or relative dimensions, nested scrolling, positioned or stacking
layout, keyboard/accessibility reordering, or browser Flexbox parity. It does
not change non-flex normal flow, source/semantic traversal, or the stable
two-crate package topology.

## Tradeoffs and risks

Reverse placement adds one bounded physical-coordinate walk and keeps the
existing item metadata/artifact ranges as the only layout state. Mapping
`justify-content` to physical edges is explicit and deterministic, but it does
not model logical writing direction or RTL. Shifting an overflowing reverse
row as a whole preserves the engine's non-negative-coordinate invariant and
root-scroll reachability, at the cost of differing from a browser's negative
initial scroll-space representation. Keeping `row` as an exact fallback makes
the new property low risk for existing fixtures and leaves unsupported column
layout visible through diagnostics.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: bounded
  `FlexDirectionValue`, non-inherited computed style, cascade, inline
  declarations, and supported-value diagnostics;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: reverse-row
  physical placement, margin-aware reverse walk, justification edge mapping,
  overflow translation, and shared subtree artifact coordinates;
- `crates/glass-browser/tests/native_engine.rs`: parser/cascade, row
  equivalence, reverse ordering, margins/gap/justification/alignment,
  overflow/root-scroll, hit-testing/paint, and normal-flow fallback;
- architecture/analysis/plan records and the issue #40 checkpoint after the
  implementation evidence is complete.

## Verification

- CSS unit tests cover accepted values, defaulting, rejected values,
  non-inheritance, selector cascade, and inline precedence;
- integration tests cover `row` equivalence, reverse visual ordering, stable
  order ties, hidden-item filtering, physical margins, gap, all four bounded
  justification values, cross-axis alignment, descendant artifacts, hit
  testing, root overflow, non-negative coordinates, and fallback preservation;
- the full native integration suite, strict default/native Clippy, formatting,
  whitespace, and documentation validators pass;
- implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed;
- exact regenerable Cargo outputs are reclaimed after all validation without
  terminating long-lived Glass processes.

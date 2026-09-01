---
id: native-engine-068
scope: glass-browser/native-engine/flex-cross-axis-alignment
status: active
depends-on: [native-engine-067]
---

# Native engine 068: bounded flex cross-axis alignment

## Objective

Add a bounded, deterministic `align-items` property to the existing eligible
single-row flex layout. This slice changes the vertical placement of visual
flex items while preserving the existing horizontal row, semantic/source
order, and two-crate package boundary.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. The implementation composes with the completed
`native-engine-064` through `native-engine-067` flex-row, gap, justification,
and visual-order slices.

## Contract

The native CSS grammar accepts `align-items` only as the non-inherited
keywords `flex-start`, `center`, or `flex-end`. The default used value remains
`flex-start`, preserving the pre-068 row geometry. Unsupported keywords such
as `stretch`, `baseline`, `normal`, `start`, and `end` produce the existing
bounded unsupported-value diagnostic and use the default fallback.

For a block-level `display:flex` container that passes the existing eligible
single-row gate:

- visible direct element children retain the existing sorted visual order,
  fixed widths, horizontal margins, `gap`, and `justify-content` positions;
- each item is first laid out at the line's top edge so its complete subtree
  height and artifact ranges are known;
- when the parent has an explicit `height`, its resolved content-box height
  (including the existing content-box/border-box, padding, border, and
  height-bound handling) is the cross-axis line size;
- when the parent has no explicit `height`, the line size is the maximum
  visible item outer height, including vertical margins, so auto-height rows
  preserve their existing height while shorter items can align within that
  line;
- for each item, `flex-start` applies zero offset, `center` applies
  `floor(max(line_size - item_outer_height, 0) / 2)`, and `flex-end` applies
  the full non-negative remainder; vertical margins are part of the item outer
  height and the item starts after its top margin;
- the computed offset shifts the item's complete layout-box and text-artifact
  ranges together, so descendant geometry, display-list paint, viewport
  projection, root overflow, and point hit testing consume the same result;
- overflow never creates a negative coordinate, and explicit heights smaller
  than an item retain the existing top-edge overflow behavior;
- semantic DOM traversal, source text, node references, accessibility source
  order, horizontal ordering, and keyboard order remain unchanged;
- an ineligible flex container continues to use normal-flow fallback and does
  not apply `align-items` to meaningful text, `display:contents`, or visible
  `<br>` content.

The default and `flex-start` paths must be coordinate- and paint-equivalent to
the completed 067 row behavior. The property is visual layout state, not a
new semantic or interaction ordering model.

## Explicit exclusions

This slice does not add `stretch`, baseline alignment, `align-content`,
`place-items`, `safe`/`unsafe` modifiers, logical `start`/`end`, cross-axis
auto margins, flex growth/shrink/basis, wrapping, reverse or column
direction, multiple lines, percentage or relative heights, nested scrolling,
stacking-context parity, keyboard/tab-order changes, accessibility
reordering, or browser Flexbox conformance. A parent with no explicit height
does not use a later min-height expansion as an additional alignment line
size; that follow-on requires its own contract.

## Tradeoffs and risks

Laying each item out once at a provisional top edge and then translating its
bounded artifact ranges adds one deterministic O(n) shift pass without
introducing a second layout owner or a speculative clone of the DOM. It keeps
auto-height rows useful and aligns nested descendants, but requires the
layout builder to preserve contiguous box/text ranges for each item. Integer
floor-centering avoids fractional coordinates and gives the extra pixel to the
bottom side. The limited keyword set is smaller than browser CSS and leaves
`stretch` visibly unsupported rather than silently pretending to implement it.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: bounded
  `AlignItemsValue`, non-inherited computed style, cascade, inline declarations,
  and supported-value diagnostics;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: resolved flex
  line height, provisional child layout, deterministic cross-axis offsets, and
  subtree artifact translation;
- `crates/glass-browser/tests/native_engine.rs`: parser/cascade, explicit and
  auto-height rows, margin/overflow behavior, descendant paint/hit-test
  alignment, and normal-flow fallback;
- `docs/architecture/native-engine.md`, `docs/plan/analysis/native-engine.md`,
  `docs/plan/README.md`, and this task file: contract and evidence records.

## Verification

- CSS unit tests cover accepted keywords, defaulting, rejected keywords, and
  non-inheritance/cascade precedence.
- Integration tests cover flex-start equivalence, center/end placement,
  auto-height line sizing, explicit content-box/border-box height, margins,
  descendant geometry, display-list/text movement, hit testing, overflow, and
  normal-flow fallback.
- The full native integration suite, strict default/native Clippy, formatting,
  whitespace, and documentation validators pass.
- Implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed.
- Exact regenerable Cargo outputs are reclaimed after all validation without
  terminating long-lived Glass processes.

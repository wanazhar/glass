---
id: native-engine-070
scope: glass-browser/native-engine/flex-wrap
status: in-progress
depends-on: [native-engine-069]
---

# Native engine 070: bounded flex wrapping

## Objective

Add a bounded, deterministic `flex-wrap` property to the existing eligible
single-row flex layout. This slice adds physical multi-line formation for
`wrap` while preserving the completed 064 through 069 item filtering, visual
order, fixed widths, physical margins, one-value `gap`, justification,
cross-axis alignment, shared artifact geometry, root scrolling, and the
two-crate package boundary.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. The implementation composes with the completed
`native-engine-064` through `native-engine-069` flex-row, gap, justification,
visual-order, cross-axis-alignment, and physical-direction slices.

## Contract

The native CSS grammar accepts `flex-wrap` only as the non-inherited keywords
`nowrap` and `wrap`. The default used value is `nowrap`, preserving the
pre-070 geometry. `wrap-reverse`, logical values, CSS-wide keywords, and
malformed values produce the existing bounded unsupported-value diagnostic and
use the `nowrap` fallback.

For a block-level `display:flex` container that passes the existing eligible
direct-element gate:

- `nowrap` is coordinate- and paint-equivalent to the completed 069 behavior;
- `wrap` partitions the already order-sorted visible direct element sequence
  into physical main-axis lines using each item's measured outer main size
  (fixed item width plus physical left/right margins) and the existing
  one-value gap;
- an item moves to the next line only when it would exceed the available
  content width and the current line already contains an item. An item wider
  than the available width occupies one line by itself; it is never shrunk and
  never receives a negative coordinate;
- whitespace-only direct text remains ignored by the eligible flex path.
  Meaningful direct text, `display:contents`, visible `<br>`, hidden items, and
  other existing fallback cases retain their documented behavior: hidden
  items are omitted, while an ineligible container uses normal-flow fallback;
- each line independently reuses the existing bounded
  `justify-content:flex-start|center|flex-end|space-between` policy. The
  configured `gap` remains the minimum main-axis separation, and positive free
  space is distributed only across gaps for `space-between`;
- `row` lays each line's order-sorted visual sequence from the physical left
  edge. `row-reverse` lays each line's sequence from the physical right edge
  using the completed 069 margin-aware walk and overflow translation. Direction
  changes physical placement only; it does not reverse the line sequence or
  semantic/source order;
- each line's cross-axis size is the maximum outer height of its visible items.
  Existing `align-items:flex-start|center|flex-end` applies independently
  inside each line and moves each complete item subtree artifact range. Lines
  stack top-to-bottom in integer pixels with no cross-line distribution;
- an explicit parent content height still controls the flex container's own
  content box, but it does not stretch wrapped lines or add implicit
  `align-content` distribution. The auto content height is the sum of formed
  line heights;
- the existing item subtree boxes, text runs, display-list paint, viewport
  projection, point hit testing, and root overflow all consume the same
  line-specific coordinates. Wrapped lines contribute to root vertical
  overflow; an over-wide item contributes to horizontal overflow and remains
  reachable through the existing root scroll path;
- semantic DOM traversal, source text, node references, accessibility order,
  and keyboard order remain document order. Wrapping changes visual geometry,
  not semantic or interaction order.

The property is a bounded physical line-formation feature, not a general
Flexbox, writing-direction, or browser-conformance claim.

## Explicit exclusions

This slice does not add `wrap-reverse`, `flex-flow`, `row-gap`, `column-gap`,
multi-value or percentage gap grammar, `align-content`, `place-content`,
flex growth/shrink/basis, auto margins, intrinsic or percentage sizing,
column directions, logical `direction`/`writing-mode`, RTL text-direction
semantics, nested scrolling, positioned or stacking layout, keyboard or
accessibility reordering, or browser Flexbox parity. It does not add a second
layout owner or change non-flex normal flow, source/semantic traversal, or the
stable two-crate package topology.

## Tradeoffs and risks

Line formation adds bounded `O(n)` partitioning before the existing per-line
placement walks and retains all artifact ranges in the existing layout builder.
Using integer outer widths and a maximum outer height per line makes wrapping
deterministic and preserves the non-negative-coordinate invariant, but it
rejects fractional, percentage, intrinsic, and flex-sized behavior that a
browser would resolve. Keeping `align-content` out of scope avoids inventing a
second cross-line distribution policy; explicit container height therefore
controls the box without changing the formed line heights. Reusing the 069
reverse walk independently for each line preserves physical margin semantics,
at the cost of not modeling logical or RTL writing direction. Root vertical
scrolling becomes observable for multiple lines while over-wide items retain
the existing horizontal-scroll behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: bounded
  `FlexWrapValue`, non-inherited computed style, cascade, inline declarations,
  and supported-value diagnostics;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: deterministic
  line formation, per-line forward/reverse placement, per-line
  justification/alignment, complete subtree artifact offsets, and shared
  overflow/scroll consumers;
- `crates/glass-browser/tests/native_engine.rs`: parser/cascade, `nowrap`
  equivalence, wrapped row and row-reverse lines, order/gap/justification,
  cross-axis alignment, hidden/fallback behavior, descendant artifacts,
  vertical/horizontal overflow, hit testing, paint, and semantic-order tests;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- CSS unit tests cover accepted values, defaulting, rejected values,
  non-inheritance, selector cascade, and inline precedence;
- integration tests cover `nowrap` equivalence, deterministic line breaks,
  wide-item handling, row and row-reverse line placement, stable order ties,
  hidden-item filtering, per-line gap/justification/alignment, complete
  descendant artifact ranges, vertical and horizontal root overflow, hit
  testing, paint, and normal-flow fallback;
- the full native integration suite, strict default/native Clippy, formatting,
  whitespace, and documentation validators pass;
- implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed;
- exact regenerable Cargo outputs are reclaimed after all validation without
  terminating long-lived Glass processes.

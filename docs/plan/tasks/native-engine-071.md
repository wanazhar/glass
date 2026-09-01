---
id: native-engine-071
scope: glass-browser/native-engine/flex-align-content
status: active
depends-on: [native-engine-070]
---

# Native engine 071: bounded flex cross-line alignment

## Objective

Add a bounded, deterministic `align-content` property to the existing wrapped
flex-row layout. This slice distributes positive cross-axis free space between
the already formed physical lines while preserving the completed 064 through
070 item filtering, visual order, fixed widths, margins, gap, justification,
cross-axis item alignment, direction, shared artifact geometry, root scrolling,
and two-crate package boundary.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. This slice composes only with the completed
`native-engine-064` through `native-engine-070` flex-row, gap, justification,
visual-order, item-alignment, direction, and wrapping slices.

## Contract

The native CSS grammar accepts `align-content` only as the non-inherited
keywords `flex-start`, `center`, `flex-end`, and `space-between`. The default
used value is `flex-start`, preserving the pre-071 wrapped geometry.
`normal`, `stretch`, `space-around`, `space-evenly`, `start`, `end`, logical
values, CSS-wide keywords, safe/unsafe modifiers, and malformed values produce
the existing bounded unsupported-value diagnostic and use the `flex-start`
fallback.

For a block-level `display:flex` container that passes the existing eligible
direct-element gate and uses `flex-wrap:wrap`:

- line formation remains the completed 070 order-sorted, measured-width,
  fixed-gap partition; `align-content` does not change item widths, line
  membership, or main-axis placement;
- each line is first laid out at its existing top-to-bottom provisional origin,
  and its cross-axis size remains the maximum visible item outer height;
- when the parent has an explicit resolved content-box height larger than the
  sum of formed line heights, the positive remainder is distributed as follows:
  `flex-start` leaves all remainder after the last line, `center` adds
  `floor(remainder / 2)` before the first line, `flex-end` puts the full
  remainder before the first line, and `space-between` distributes it across
  the gaps between lines using the existing integer quotient/remainder policy;
- when the parent has no explicit content height, or its resolved content
  height is no larger than the formed lines, no positive distribution is
  created and line origins remain stacked without negative offsets;
- `align-content` applies to the complete line artifact ranges after
  `align-items` has placed each item within its own line. Direct item boxes,
  descendants, text runs, display-list entries, viewport projection, point hit
  testing, root overflow, and capture therefore consume identical final
  coordinates;
- a wrapped row with one formed line may use `center` or `flex-end` within an
  explicitly taller content box, while `space-between` has no inter-line gap
  to distribute; this behavior is deterministic and remains bounded to the
  owned line layout;
- explicit heights smaller than the formed lines retain the existing overflow
  behavior. No line is assigned a negative coordinate, and over-wide items
  retain the completed 070 horizontal root-scroll reachability;
- `nowrap` remains coordinate- and paint-equivalent to 070 and ignores
  `align-content`; ineligible containers retain normal-flow fallback;
- semantic DOM traversal, source text, node references, accessibility order,
  and keyboard order remain document order. Cross-line alignment changes only
  visual geometry.

The property is bounded cross-line placement, not general Flexbox or browser
conformance.

## Explicit exclusions

This slice does not add `stretch`, `space-around`, `space-evenly`, logical
alignment, `place-content`, `flex-flow`, row/column cross-axis gaps,
`row-gap`, `column-gap`, flex growth/shrink/basis, auto margins, column
directions, `wrap-reverse`, logical direction/RTL, intrinsic or percentage
sizing, nested scrolling, positioned or stacking layout, keyboard or
accessibility reordering, or browser Flexbox parity. It does not change
non-flex normal flow, source/semantic traversal, or the stable two-crate
package topology.

## Tradeoffs and risks

The slice retains provisional line records and translates complete existing
artifact ranges after all item heights are known. That keeps one layout owner
and lets `align-items` remain independent, but it adds a bounded second pass
over formed lines and their recorded item ranges. Integer remainder allocation
is reproducible but does not model fractional/subpixel flex-line distribution.
Applying distribution only to positive free space avoids negative coordinates
and preserves the existing overflow behavior when explicit height is too small.
The default `flex-start` and `nowrap` paths remain unchanged, reducing risk to
existing fixtures, while `space-between` intentionally covers only inter-line
gaps and does not introduce a separate cross-axis gap property.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: bounded
  `AlignContentValue`, non-inherited computed style, cascade, inline
  declarations, and supported-value diagnostics;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: provisional
  line records, explicit-content-height free-space calculation, deterministic
  line offsets, complete line artifact translation, and shared overflow/scroll
  consumers;
- `crates/glass-browser/tests/native_engine.rs`: parser/cascade, default and
  `nowrap` equivalence, explicit-height `center`/`flex-end`/`space-between`,
  no-extra-space behavior, one-line behavior, descendant artifacts,
  vertical overflow, hit testing, paint, and semantic-order tests;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- [ ] CSS unit tests cover accepted values, defaulting, rejected values,
  non-inheritance, selector cascade, and inline precedence;
- [ ] integration tests cover wrapped line offsets for all supported values,
  explicit/auto/smaller heights, one-line and multi-line cases, `nowrap`
  equivalence, descendants, root overflow, hit testing, paint, and fallback;
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

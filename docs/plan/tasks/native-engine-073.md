---
id: native-engine-073
scope: glass-browser/native-engine/flex-align-content-space-evenly
status: active
depends-on: [native-engine-072]
---

# Native engine 073: bounded flex cross-line space-evenly

## Objective

Extend the completed 071 and 072 wrapped-row `align-content` owner with one
additional bounded value, `space-evenly`, while preserving line formation,
visual order, fixed widths, margins, gap, justification, cross-axis item
alignment, direction, wrapping, shared artifact geometry, root scrolling, and
the two-crate package boundary.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. This slice composes only with the completed
`native-engine-064` through `native-engine-072` flex-row, gap, justification,
visual-order, item-alignment, direction, wrapping, and bounded cross-line
alignment slices.

## Contract

The native CSS grammar adds the non-inherited keyword `space-evenly` to
`align-content`. The existing accepted values remain
`flex-start|center|flex-end|space-between|space-around`; the default remains
`flex-start`. `stretch`, `normal`, logical values, CSS-wide keywords,
safe/unsafe modifiers, and malformed values continue to produce the existing
bounded unsupported-value diagnostic and use the `flex-start` fallback.

For a block-level `display:flex` container that passes the existing eligible
direct-element gate and uses `flex-wrap:wrap`:

- line formation, line sizes, item sizing, per-line `align-items`, main-axis
  placement, and complete line artifact ownership remain unchanged;
- when explicit resolved content-box height has positive remainder after the
  formed line heights, `space-evenly` gives every leading, inter-line, and
  trailing slot the same integer distribution. The line-start offset for line
  `i` is `floor(remainder * (i + 1) / (line_count + 1))`, with saturating
  bounded arithmetic and floor rounding;
- the formula allocates space before the first line, between every pair of
  lines, and after the final line without creating a negative coordinate;
- auto-height and undersized content boxes remain stacked as before, and
  overflow remains owned by the existing root projection and scroll paths;
- `space-evenly` applies to complete line artifact ranges after `align-items`.
  Direct item boxes, descendants, text runs, display-list entries, viewport
  projection, point hit testing, root overflow, and capture consume the same
  shifted coordinates;
- a single formed line receives one equal slot before it and one after it;
  `nowrap` ignores `align-content` and remains coordinate- and paint-equivalent
  to 072;
- semantic DOM traversal, source text, node references, accessibility order,
  and keyboard order remain document order. The value changes visual geometry
  only.

The property remains bounded cross-line placement, not general Flexbox or
browser conformance.

## Explicit exclusions

This slice does not add `stretch`, `place-content`, cross-axis gaps,
`row-gap`, `column-gap`, flex growth/shrink/basis, auto margins, column
directions, `wrap-reverse`, logical direction/RTL, intrinsic or percentage
sizing, nested scrolling, positioned or stacking layout, keyboard or
accessibility reordering, or browser Flexbox parity. It does not change
non-flex normal flow, source/semantic traversal, or the stable two-crate
package topology.

## Tradeoffs and risks

The implementation uses integer document pixels and floors each ideal slot
boundary. That makes the result reproducible and bounded, but it cannot model
fractional or subpixel Flexbox distribution. Saturating `u64` intermediate
arithmetic prevents the free-space/index product and denominator construction
from wrapping before conversion to bounded `u32` geometry. The default and
all previously supported values retain their existing paths; only the new
parser value selects the `space-evenly` formula.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: add the bounded
  `SpaceEvenly` computed value, cascade, inline declaration, and diagnostics;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: add the
  saturating integer line-offset formula to the existing complete-artifact
  second pass;
- `crates/glass-browser/tests/native_engine.rs`: parser/cascade acceptance and
  rejection, single- and multi-line explicit-height geometry, auto/smaller
  heights, `nowrap` equivalence, descendants, paint, hit testing, overflow,
  and semantic-order coverage;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- [ ] CSS unit tests cover accepted `space-evenly`, defaulting, rejected
  values, non-inheritance, selector cascade, and inline precedence;
- [ ] integration tests cover the saturating offset formula for one and
  multiple lines, explicit/auto/smaller heights, `nowrap` equivalence,
  descendants, root overflow, hit testing, paint, and semantic/source-order
  fallback;
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

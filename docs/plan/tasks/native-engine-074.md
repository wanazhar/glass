---
id: native-engine-074
scope: glass-browser/native-engine/flex-wrap-reverse
status: active
depends-on: [native-engine-073]
---

# Native engine 074: bounded flex wrap-reverse

## Objective

Extend the completed 070 through 073 wrapped-row owner with the physical
cross-axis reversal represented by `flex-wrap:wrap-reverse`, while preserving
the exact two-crate package boundary, the default-off native feature, line
formation, visual order, fixed widths, margins, gap, justification,
cross-axis item alignment, shared artifact geometry, root scrolling, and
semantic/source order.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. This slice composes only with the completed
`native-engine-064` through `native-engine-073` flex-row, gap, justification,
visual-order, item-alignment, direction, wrapping, and cross-line alignment
slices.

## Contract

The native CSS grammar adds the non-inherited keyword `wrap-reverse` to
`flex-wrap`. The existing accepted values remain `nowrap|wrap`; the default
remains `nowrap`. `flex-flow`, CSS-wide keywords, malformed values, and
unsupported directions continue to produce the existing bounded unsupported
value diagnostic and use the `nowrap` fallback where applicable.

For a block-level `display:flex` container that passes the existing eligible
direct-element gate, uses the existing fixed-width row direction, and selects
`flex-wrap:wrap-reverse`:

- item sizing, stable `(order, source_index)` sorting, line membership, gap,
  main-axis justification, and per-line `align-items` remain the completed
  070-through-073 behavior;
- source-order line formation remains unchanged, but the first formed line is
  placed at the physical cross-axis end and later formed lines proceed toward
  the cross-axis start;
- the existing `align-content` value is measured from that reversed
  cross-axis start. For a line record with relative top `line.y - y`, height
  `line.height`, resolved content height `line_content_height`, and computed
  line offset `line_offset`, the bounded physical top is
  `y + line_content_height - (line.y - y + line.height) - line_offset`, using
  saturating arithmetic. This makes `flex-start`, `center`, `flex-end`,
  `space-between`, `space-around`, and `space-evenly` reverse their physical
  stacking without changing their deterministic integer rounding;
- the line's complete artifact range is translated by a signed bounded
  document-pixel delta from its provisional top. Direct item boxes, nested
  descendants, text runs, display-list entries, viewport projection, point
  hit testing, root overflow, scrolling, and capture all consume the same
  final coordinates;
- explicit heights with positive remainder keep the existing 071/072/073
  line-offset formulas, auto-height rows reverse their formed line stack, and
  undersized content keeps bounded saturating geometry and existing overflow
  ownership without inventing nested scrolling;
- `nowrap` remains equivalent to the current no-wrap geometry, semantic DOM
  traversal and accessibility/source order remain document order, and
  `row-reverse` remains an independent main-axis direction.

The property remains bounded physical cross-axis placement, not general
Flexbox or browser conformance.

## Explicit exclusions

This slice does not add `flex-flow`, column directions, logical direction or
RTL mapping, cross-axis gaps, multi-value or percentage gaps, flex
growth/shrink/basis, auto margins, `align-content:stretch`, `place-content`,
intrinsic or percentage sizing, nested scrolling, positioned or stacking
layout, keyboard/accessibility reordering, JavaScript, or browser Flexbox
parity. It does not change non-flex normal flow, source/semantic traversal,
the stable two-crate package topology, or the default production Chromium/CDP
path.

## Tradeoffs and risks

The line records are still laid out once at provisional top-to-bottom origins,
then translated. A signed translation is necessary because a reversed line
can finish above its provisional origin; clamping each coordinate at zero
keeps the existing unsigned bounded geometry contract. This adds a small
coordinate helper and a second-pass concern, but avoids a second layout owner
or re-running child layout. Integer and saturating arithmetic remains
deterministic, while fractional cross-axis distribution and standards-level
overflow behavior remain outside the claim. Because wrap reversal is a
physical cross-axis operation, it is intentionally independent from
`row-reverse` and does not reorder semantic nodes.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: add the bounded
  `WrapReverse` computed value, parser, cascade, inline declaration, and
  diagnostics coverage;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: treat
  `wrap-reverse` as wrapped, compute reversed physical line tops, and apply a
  signed artifact-preserving vertical translation;
- `crates/glass-browser/tests/native_engine.rs`: parser/cascade acceptance and
  rejection, explicit-height line reversal across all existing
  `align-content` values, auto/smaller heights, `nowrap` equivalence,
  descendants, paint, hit testing, root overflow, scrolling, and
  semantic/source-order coverage;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- [ ] CSS unit tests cover accepted `wrap-reverse`, defaulting, rejected
  values, non-inheritance, selector cascade, and inline precedence;
- [ ] integration tests cover reversed line membership geometry, all bounded
  `align-content` values, signed artifact translation, explicit/auto/smaller
  heights, `nowrap` equivalence, descendants, root overflow, scrolling,
  hit testing, paint, and semantic/source-order preservation;
- [ ] the full native integration suite, strict default/native Clippy,
  formatting, whitespace, and documentation validators pass;
- [ ] implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed;
- [ ] exact regenerable Cargo outputs are reclaimed after all validation
  without terminating long-lived Glass processes.

## Completion evidence

Pending implementation and validation.

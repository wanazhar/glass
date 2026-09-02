---
id: native-engine-079
scope: glass-browser/native-engine/flex-grow
status: active
depends-on: [native-engine-078]
---

# Native bounded flex-grow allocation

## Objective

Extend the existing bounded row-flex geometry owner with deterministic positive
free-space allocation for eligible flex items. The slice makes common fixed
width rows adapt their item widths without introducing a second layout engine,
fractional metrics, or a new workspace crate/dependency.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `flex-grow` as a non-negative decimal integer
from `0` through `1024`. The property is non-inherited, defaults to `0`, and
uses the existing selector specificity, source order, and inline precedence.
Invalid, negative, fractional, unit-bearing, CSS-wide, and out-of-range values
remain unsupported and are reported through the existing bounded diagnostic
path without replacing a valid value.

For an eligible block-level `display:flex` row or `row-reverse` container,
the existing computed outer width after padding, borders, min/max constraints,
and margins is each item's flex base width. Wrapping and line formation still
use those base widths and the resolved column gap. After a line is formed, if
its base items and gaps leave positive free space and at least one item has a
positive grow factor, that free space is allocated by grow-weighted integer
shares. The prefix-floor remainder policy assigns every pixel deterministically
in visual/source order; arithmetic uses bounded `u64` intermediates.

Existing bounded `max-width` constraints cap grown outer widths. A capped item
is frozen and any unallocated remainder is redistributed among the remaining
positive grow factors until no eligible item can accept more space. Existing
minimum constraints remain part of each base width. If free space is zero or
negative, or all grow factors are zero, the prior fixed-width behavior remains;
there is no shrink pass.

Growth occurs before `justify-content`, so a line whose positive free space was
fully consumed by grow factors presents zero positive free space to the
existing justification owner. Gap, margins, row/cross-line alignment,
row-reverse placement, wrapping, overflow, paint, raster, viewport projection,
hit testing, scrolling, capture, and semantic/source order all consume the
same final item widths and coordinates. A grown width is passed into child
layout as the item's outer width so descendants and content rectangles observe
the same geometry.

This slice explicitly excludes `flex-shrink`, `flex-basis`, the `flex`
shorthand, fractional grow factors, auto margins, column directions,
percentage/intrinsic sizing changes, multiple independent flex formatting
contexts, and browser Flexbox conformance. It does not alter semantic order or
make native-engine selection implicit.

## Tradeoffs

- Integer grow factors and prefix-floor shares keep the current bounded
  coordinate model deterministic and cheap, but decimal CSS grow values remain
  visibly unsupported.
- Applying growth after line formation makes wrapped rows predictable and
  preserves the current line owner, but it does not reflow items based on
  grown widths; wrapping remains a base-size decision for this slice.
- Freezing max-constrained items and redistributing their remainder preserves
  the existing min/max contract, but adds a bounded iterative sizing pass.
- Forcing the final outer width into child layout keeps descendants, paint,
  hit testing, and overflow coherent, but it intentionally treats the current
  fixed-width box model as the flex base-size owner rather than implementing
  full CSS used-value resolution.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- parser, bounds, invalid fallback, selector specificity, source order, inline
  precedence, and non-inheritance are covered by CSS unit tests;
- one-line and wrapped rows cover weighted growth, zero/negative free space,
  row-reverse, gap, margins, justify interaction, stable ordering, and
  max-width freeze/redistribution;
- grown child width reaches content rectangles, descendants, display-list,
  raster, overflow, viewport projection, hit testing, scrolling, capture, and
  source/semantic order;
- existing native integration/library tests and all strict feature/documentation
  gates remain green;
- the implementation, documentation, and issue #40 checkpoints are committed
  locally before the next slice; remote CI is not claimed until push.

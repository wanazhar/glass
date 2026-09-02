---
id: native-engine-075
scope: glass-browser/native-engine/flex-align-content-stretch
status: active
depends-on: [native-engine-074]
---

# Native engine 075: bounded flex cross-line stretch

## Objective

Extend the completed wrapped-row `align-content` owner with explicit
`stretch`, using the same two-crate boundary, default-off native feature,
deterministic integer geometry, and shared layout/paint/hit-test/scroll/capture
consumers. This slice composes with both normal and `wrap-reverse` physical
cross-axis stacking without adding a second layout owner.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. This slice composes only with the completed
`native-engine-064` through `native-engine-074` flex-row, gap, justification,
visual-order, item-alignment, direction, wrapping, cross-line distribution,
and wrap-reverse slices.

## Contract

The native CSS grammar adds the non-inherited keyword `stretch` to
`align-content`. The existing accepted values remain
`flex-start|center|flex-end|space-between|space-around|space-evenly`; the
default remains `flex-start`. `normal`, `place-content`, logical values,
CSS-wide keywords, safe/unsafe modifiers, and malformed values continue to
produce the existing bounded unsupported-value diagnostic and use the
`flex-start` fallback.

For a block-level `display:flex` container that passes the existing eligible
direct-element gate, has a fixed-width row direction, and selects
`flex-wrap:wrap` or `flex-wrap:wrap-reverse`:

- item sizing, stable `(order, source_index)` sorting, line membership, gap,
  main-axis justification, and the selected physical line stacking remain the
  completed 070-through-074 behavior;
- only a positive remainder from an explicit resolved content-box height after
  the originally formed line heights is distributable. If the height is
  auto, absent, or undersized, line heights and existing overflow behavior are
  unchanged;
- the remainder is divided by the formed line count using integer floor
  division. Every line grows by the quotient, and the first formed lines in
  source/order-sorted formation order receive one additional pixel each until
  the remainder is exhausted. The resulting line records are then rebuilt
  top-to-bottom before the existing second pass;
- `align-items:flex-start|center|flex-end` is evaluated against each expanded
  line height. For `wrap-reverse`, the expanded line records are reflected from
  the physical cross-axis end using the 074 signed artifact translation, so
  the extra line space follows the same deterministic physical line order;
- direct item boxes, nested descendants, text runs, display-list entries,
  viewport projection, point hit testing, root overflow, scrolling, and
  capture consume the same final line and item coordinates;
- semantic DOM traversal, source text, node references, accessibility order,
  and keyboard order remain document order. `nowrap` ignores `align-content`
  and remains unchanged.

The property remains bounded line-box expansion, not general Flexbox or
browser conformance.

## Explicit exclusions

This slice does not add implicit `align-content:normal`, `place-content`,
cross-axis gaps, `row-gap`, `column-gap`, flex growth/shrink/basis, auto
margins, column directions, logical direction/RTL mapping, intrinsic or
percentage sizing, nested scrolling, positioned or stacking layout,
keyboard/accessibility reordering, JavaScript, or browser Flexbox parity. It
does not change non-flex normal flow, the default production Chromium/CDP
path, the stable two-crate package topology, or the existing unsupported-value
diagnostic contract.

## Tradeoffs and risks

Stretch changes line heights rather than merely moving lines. That is the
necessary model for line-box expansion, and it lets existing `align-items`
logic place shorter items within the newly available space. The first formed
lines receive integer remainder pixels, making output reproducible but
introducing a deliberate source-order rounding bias. Rebuilding line records
before the existing artifact pass avoids re-running child layout and keeps
descendants under one coordinate owner. The implementation does not attempt
fractional distribution, intrinsic sizing, or standards-level overflow
behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: add the bounded
  `Stretch` computed value, parser, cascade, inline declaration, and
  diagnostics coverage;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: expand formed
  line heights by deterministic integer shares and rebuild their provisional
  origins before normal/reverse line translation and `align-items` placement;
- `crates/glass-browser/tests/native_engine.rs`: parser/cascade acceptance and
  rejection, explicit-height normal and wrap-reverse geometry, remainder
  rounding, auto/smaller heights, `nowrap` equivalence, descendants, paint,
  hit testing, root overflow, scrolling, and semantic/source-order coverage;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- [ ] CSS unit tests cover accepted `stretch`, defaulting, rejected values,
  non-inheritance, selector cascade, and inline precedence;
- [ ] integration tests cover normal and wrap-reverse line expansion,
  deterministic remainder pixels, per-line item alignment, explicit/auto/
  smaller heights, `nowrap` equivalence, descendants, root overflow,
  scrolling, hit testing, paint, and semantic/source-order preservation;
- [ ] the full native integration suite, strict default/native Clippy,
  formatting, whitespace, and documentation validators pass;
- [ ] implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed;
- [ ] exact regenerable Cargo outputs are reclaimed after all validation
  without terminating long-lived Glass processes.

## Completion evidence

Pending implementation and validation.

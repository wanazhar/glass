---
id: native-engine-077
scope: glass-browser/native-engine/flex-row-gap
status: active
depends-on: [native-engine-076]
---

# Native engine 077: bounded flex row-gap

## Objective

Add explicit cross-line `row-gap` spacing to the completed wrapped fixed-width
flex-row owner. The slice must feed the same line records that already own
`align-content`, `wrap-reverse`, `align-items`, overflow, display-list,
hit-testing, scrolling, and capture output. It must remain inside
`glass-browser`, behind the default-off `native-engine` feature, and preserve
the two-installable-crate workspace boundary.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. This slice composes with the completed `native-engine-064`
through `native-engine-076` flex-row, main-axis gap, justification,
visual-order, item-alignment, direction, wrapping, cross-line distribution,
wrap-reverse, stretch, and normal slices.

## Contract

The bounded native CSS grammar adds the non-inherited `row-gap` property as a
single non-negative integer-pixel value in the existing
`0..=MAX_NATIVE_VIEWPORT_DIMENSION` range. An explicit valid `row-gap` applies
only to eligible block-level `display:flex` containers using row direction and
`flex-wrap:wrap` or `wrap-reverse`:

- formed lines retain the existing stable `(order, source_index)` membership,
  item widths, main-axis `gap`, `justify-content`, and per-line
  `align-items` behavior;
- a row gap is inserted between adjacent formed line boxes before
  `align-content` computes positive explicit cross-axis free space. The
  occupied cross size is therefore `sum(line heights) + row_gap * (line_count -
  1)`, with saturating integer arithmetic;
- `align-content:flex-start|center|flex-end|space-between|space-around|
  space-evenly|stretch|normal` distributes only the remainder left after
  those explicit row gaps. `stretch` and `normal` expand line boxes without
  consuming or duplicating the explicit gap; `space-between` adds its
  distributed remainder in addition to the explicit gap;
- `wrap-reverse` reflects the complete row records, including the explicit
  row-gap space, through the existing signed artifact translation. Direct
  boxes, descendants, text runs, display-list commands, viewport projection,
  hit testing, root overflow, scrolling, and capture all consume the final
  coordinates;
- `row-gap` is non-inherited. A child does not receive its parent's value, and
  selector and inline cascade precedence remain the existing bounded rules;
- omitted or invalid `row-gap` remains `0`. Existing `gap` behavior remains
  the established main-axis item spacing for this checkpoint, so current
  single-row and horizontal wrap geometry does not silently change;
- non-flex flow, `nowrap`, auto-height rows without a second line, and rows
  whose explicit height is smaller than their occupied content preserve their
  existing geometry except where an actual inter-line row gap is present;
- semantic DOM traversal, source text, node references, accessibility order,
  and keyboard order remain document order.

The property is a bounded explicit cross-line spacing primitive, not a claim
of complete CSS `gap` shorthand, grid, or browser Flexbox conformance.

## Explicit exclusions

This slice does not add `column-gap`, multi-value `gap`, percentage or
fractional gap values, implicit gap expansion from the existing `gap`
property, flex growth/shrink/basis, auto margins, column directions, logical
direction/RTL mapping, intrinsic sizing, percentage sizing, nested scrolling,
positioned or stacking layout, JavaScript, or browser parity. It does not
change the default production Chromium/CDP path, non-flex normal flow, the
stable two-crate package topology, or the existing default `row-gap:0`
fallback.

## Tradeoffs and risks

Adding row-gap to the occupied line size before `align-content` is important:
otherwise centered, evenly distributed, or stretched rows would double-count
the same cross-axis space and produce visibly incorrect line origins. Keeping
the existing one-value `gap` as main-axis-only for this checkpoint avoids
changing every previously certified wrapped-row golden, but it knowingly
leaves the CSS shorthand incomplete. A later shorthand/column-gap slice can
choose a declaration-order model deliberately instead of hiding that
compatibility change inside this geometry patch. Saturating arithmetic keeps
large bounded values safe, while integer spacing preserves deterministic
pixel output at the cost of the same deliberate rounding behavior already
used by `align-content`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: add the bounded
  computed `row_gap` value, getter, parser, cascade, inline declaration, and
  diagnostics coverage without inheriting it;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: insert row-gap
  space between provisional wrapped line records and include it once in
  cross-axis occupied-size/free-space calculations;
- `crates/glass-browser/tests/native_engine.rs`: cover parser/cascade
  acceptance, invalid fallback, non-inheritance, normal and wrap-reverse
  geometry, interaction with all completed `align-content` values, explicit
  height overflow, paint, hit testing, and semantic/source-order preservation;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- [ ] CSS unit tests cover accepted bounded pixels, zero, rejected units/
  ranges, non-inheritance, selector cascade, inline precedence, and
  diagnostics;
- [ ] integration tests cover two-line row-gap geometry, wrap-reverse,
  `align-content` remainder math, omitted/invalid fallback, auto/smaller
  heights, no-op single-line behavior, descendants, paint, hit testing,
  overflow/scrolling, and semantic/source order;
- [ ] the full native integration suite, strict default/native Clippy,
  formatting, whitespace, rustdoc, and repository documentation validators
  pass;
- [ ] implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed;
- [ ] exact regenerable Cargo outputs are reclaimed after all validation
  without terminating long-lived Glass processes.

## Completion evidence

Pending implementation and validation.

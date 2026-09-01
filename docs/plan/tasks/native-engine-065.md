---
id: native-engine-065
scope: glass-browser/native-engine/flex-row-gap
status: active
depends-on: [native-engine-064]
---

# Native bounded flex-row gap

## Objective

Add one bounded fixed-pixel `gap` value to the existing native
`display:flex` single-row layout. The gap must affect only eligible flex rows
and must continue to use the existing box-model, layout, paint, semantic,
hit-test, scroll, and capture owners.

## Contract

The native CSS grammar accepts `gap: Npx` where `N` is a bounded non-negative
integer pixel value. The value is a non-inherited computed presentation value
with the existing cascade and inline-style precedence. Negative, percentage,
unitless, multi-value, keyword, and malformed values remain unsupported and
retain the existing diagnostic behavior. `row-gap`, `column-gap`, and the
`flex` shorthand remain unsupported.

For an eligible block-level `display:flex` container from
[native-engine-064](native-engine-064.md), the gap is inserted exactly once
between each pair of visible, rendered direct element items in source order.
There is no leading or trailing gap. Hidden and `display:none` direct elements
do not consume a pair position, so they do not create a visible gap. Item
widths, margins, y-origins, heights, source order, and nested layout retain the
064 rules; the gap adds only bounded horizontal cursor distance.

The row extent includes item outer widths, horizontal margins, and inserted
gaps. Existing root overflow measurement and horizontal scrolling expose that
extent without shrinking or wrapping items. Auto height, explicit height,
content-box/border-box sizing, ancestor clips, opacity groups, paint order,
semantic projection, point hit testing, and PNG capture remain owned by the
existing paths.

If meaningful direct text, `display: contents`, or a visible `<br>` makes the
container ineligible, the existing normal-flow fallback is used and `gap` has
no effect. No anonymous flex items are synthesized.

This is spacing for the bounded single forward row, not CSS Flexbox gap
conformance. It does not add row/column direction, wrapping, distributed free
space, cross-axis alignment, or nested scrolling.

## Tradeoffs

- Accepting one fixed-pixel value makes common card/control spacing observable
  with a small, deterministic parser and layout change, but excludes the full
  two-axis and keyword grammar.
- Applying the gap only between rendered items avoids phantom spacing around
  hidden content, but differs from any future anonymous-item or independent
  row/column model.
- Keeping the gap in the existing integer cursor preserves shared geometry and
  overflow consumers, while all flex distribution and wrapping remain explicit
  future slices.
- Recognizing `gap` outside flex rows keeps diagnostics truthful and cascade
  behavior inspectable, but the property has no visual effect in normal flow.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- CSS parsing and cascade accept one-value non-negative fixed-pixel `gap` and
  continue to diagnose unsupported gap variants;
- eligible flex rows insert one gap only between visible rendered items and
  retain item margins, source order, nested layout, and fixed widths;
- row extent and root horizontal scrolling include gaps without shrinking or
  wrapping items;
- hidden/none items do not consume gap positions;
- ineligible flex containers preserve normal-flow text and descendants and do
  not apply the gap;
- item boxes, text artifacts, paint order, clips, semantics, point hit testing,
  root scrolling, and PNG capture share the resulting geometry;
- focused CSS/integration tests, the native suite, strict lint, formatting,
  whitespace, and documentation validators pass;
- implementation and documentation checkpoints are committed locally, issue
  #40 is updated, and exact regenerable Cargo outputs are reclaimed after all
  validation.

## Completion evidence

This design checkpoint is active. Implementation and validation evidence will
be added here before the task is marked complete.

---
id: native-engine-064
scope: glass-browser/native-engine/flex-row
status: active
depends-on: [native-engine-063]
---

# Native bounded flex row

## Objective

Add a bounded `display: flex` layout mode to the existing fixed-cell native
engine. Eligible containers should place direct element children in one
deterministic horizontal row while continuing to use the existing box model,
layout, paint, semantic, hit-test, scroll, and capture owners.

## Contract

The native CSS grammar accepts `display: flex` as a block-level flex
container. `display: block`, `inline`, `inline-block`, `contents`, and the
existing defaults retain their current behavior. `inline-flex`, grid values,
and all other flex display variants remain unsupported and retain the existing
diagnostic/fallback behavior.

An eligible flex container has only direct element children, aside from
whitespace-only direct text nodes. A visible non-whitespace direct text node,
`display: contents` child, or visible `<br>` makes the container use the
existing normal-flow fallback so source text and descendants are not dropped.
Hidden and `display:none` direct elements remain absent as they are in normal
flow. Eligible direct element children become atomic flex items in source
order; nested flex containers may use this same bounded mode.

Each item uses its existing explicit or intrinsic outer width, including
physical horizontal margins, and is laid out once at the next integer x
position in the container's content box. Items do not grow, shrink, reorder,
wrap, reverse, or distribute free space. There is no `gap`, cross-axis
stretching, alignment keyword, basis, or shorthand flex grammar in this
slice. Items start at the container's content-box top plus their top margin;
the line height and item heights remain owned by the existing box model.

The flex container retains the existing content-box/border-box width and
height rules. Its auto content height is the maximum item outer height, and an
explicit height remains authoritative. The row may extend beyond the
container's bounded content width; the existing document overflow measurement
and root horizontal scrolling expose that extent without implicit shrinking.
Vertical margins contribute to the auto height and horizontal margins
contribute to the row extent. Saturating integer arithmetic and existing
native limits remain in force.

Child layout boxes, direct text runs, paint order, opacity groups, ancestor
clips, point hit testing, root scrolling, semantic projection, and PNG
capture all consume the same origins produced by the flex row. No second
layout tree, renderer, dependency, or stable backend capability is added.
This is a deterministic single-row fixture layout, not CSS Flexbox
conformance.

## Tradeoffs

- Reusing `layout_element` for each item keeps nested paint, clipping,
  semantics, and hit testing coherent, but it deliberately omits flex grow,
  shrink, and cross-axis alignment.
- Falling back when meaningful direct text or `display: contents` is present
  protects the existing text/descendant flow contract, but those containers do
  not receive row placement until a later anonymous-item/flattening slice.
- Preserving fixed item widths makes overflow and scrolling observable and
  deterministic, but it does not match browsers' default shrink behavior.
- Supporting only a single forward row avoids a direction/alignment matrix and
  keeps build cost unchanged, while column, reverse, wrapping, gaps, and
  distributed free space remain explicit future work.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- CSS parsing accepts `display:flex` and continues to diagnose unsupported
  display variants;
- eligible direct element children are placed once in source order with
  explicit/intrinsic widths, margins, nested layout, and deterministic row
  overflow;
- explicit container height remains authoritative while auto height follows
  the maximum item outer height;
- item boxes, text artifacts, source-order paint, clips, opacity, semantics,
  point hit testing, root scrolling, and PNG capture share the same geometry;
- whitespace-only direct text is harmless, while meaningful direct text,
  `display:contents`, and visible `<br>` preserve normal-flow content;
- focused CSS/integration tests, the native suite, strict lint, formatting,
  whitespace, and documentation validators pass;
- the implementation and documentation checkpoints are committed locally,
  issue #40 is updated, and exact regenerable Cargo outputs are reclaimed
  after validation.

## Completion evidence

This design checkpoint is active. Implementation and validation evidence will
be added here before the task is marked complete.

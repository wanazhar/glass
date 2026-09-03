---
id: native-engine-094
scope: glass-browser/native-engine/flex-direction-column
status: active
depends-on: [native-engine-093]
---

# Native bounded flex-direction column

## Objective

Extend the native Flexbox boundary from horizontal rows to a bounded vertical
`column`/`column-reverse` main axis without creating a second document or
geometry owner. Reuse the existing item sorting, gap, flexible-length,
justification, cross-axis alignment, descendant, paint, hit-test, scroll, and
capture contracts where their axis is explicitly mapped.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Flexible Box Layout Module Level 1: flex flow direction](https://www.w3.org/TR/css-flexbox-1/#flex-direction)
- [CSS Flexible Box Layout Module Level 1: axis mappings](https://www.w3.org/TR/css-flexbox-1/#axis-mapping)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar and cascade accept `flex-direction:column` and
`flex-direction:column-reverse` as distinct non-inherited computed keywords.
The existing `flex-flow` parser accepts the same direction values alongside
the already bounded wrap values. Omitted direction remains `row`.

Layout uses the new direction only for an eligible bounded vertical flex
container:

- the container is `display:flex`, has only direct eligible element children,
  has a finite explicit content height, and uses `flex-wrap:nowrap`;
- each visible child has an explicit bounded height or a bounded pixel
  `flex-basis`; its width remains the existing bounded fixed/intrinsic cross
  size, and physical margins remain authoritative;
- `row-gap` is the vertical main-axis gap between items; the existing
  `column-gap` field is not consumed for a single no-wrap column line;
- existing visual `(order, source_index)` sorting occurs before placement while
  DOM and semantic/source order remain unchanged;
- `justify-content:normal|flex-start|center|flex-end|space-between|
  space-around|space-evenly|stretch` distributes the formed items along the
  vertical main axis using the existing bounded integer policies. Explicit
  `stretch` and `normal` retain their distinct computed values and use the
  existing flex-start fallback owner;
- `column` walks from physical top to bottom and `column-reverse` walks from
  physical bottom to top. Reverse placement preserves margins, gaps, item
  identity, complete descendant ranges, non-negative bounded coordinates, and
  the existing overflow/scroll behavior;
- `align-items` and per-item `align-self` reuse the existing bounded
  `flex-start|center|flex-end|stretch|normal` cross-axis semantics. Explicit
  child widths remain authoritative; an auto-width child may use the existing
  stretch width path within the finite container content width;
- existing integer `flex-grow`, base-width-weighted `flex-shrink`, and
  `flex-basis:auto|Npx` policies are applied on the vertical main axis after
  line formation and before `justify-content`. Flexible resizing changes the
  item’s outer height through the existing box owner while preserving
  descendant artifact ranges;
- the final box coordinates and complete descendant artifact ranges feed the
  existing layout, display-list, software rasterization, viewport projection,
  hit testing, scrolling, capture, and semantic/source-order consumers.

`flex-wrap:wrap` and `wrap-reverse`, an auto-height column container, multiple
vertical lines, cross-axis line packing, column-gap distribution, auto
margins, percentage/fractional/intrinsic main sizes, logical direction or
writing modes, baseline alignment, grid/block/absolute layout, and
browser-wide Flexbox conformance remain outside this slice. Such contexts
retain the established normal-flow fallback or bounded unsupported behavior;
the parser accepting a direction does not claim those layout modes.

Cascade specificity, source order, inline precedence, non-inheritance,
failure-atomic diagnostics, and the two-crate/default-off native-engine
boundary remain unchanged. No network, JavaScript, storage, new dependency,
third crate, runtime, or artifact pipeline is introduced.

## Tradeoffs

- Mapping the vertical axis into the existing flex item and artifact owners
  advances real column layouts without duplicating document state or paint
  consumers, at the cost of deliberately excluding multi-line columns until
  a separate cross-axis line contract exists.
- Requiring explicit container height makes free-space and reverse placement
  deterministic and prevents an incomplete intrinsic-sizing algorithm from
  masquerading as browser behavior; auto-height columns retain the established
  fallback.
- Reusing the current integer grow/shrink policies keeps row and column
  rounding consistent, while the bounded fixed-pixel scope does not model
  percentage, fractional, auto-margin, or font-metric contributions.
- Keeping the computed `column` keywords distinct preserves CSS provenance even
  when unsupported combinations fall back, and requires parser, cascade,
  `flex-flow`, diagnostics, layout, and complete-artifact regression coverage.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The certification gate will include focused parser/cascade tests, focused
column/column-reverse layout and artifact tests, the full native integration
and feature-enabled library suites, strict all-feature and no-default-feature
Clippy, warning-denied workspace rustdoc, locked `glass-dev` binary
compilation, static documentation/release validators, live documentation
coverage with explicit temporary binaries, and exact isolated-target cleanup.
Remote CI remains pending because `main` is local-only and has not been
pushed.

---
id: native-engine-156
scope: glass-browser/native-engine/cascade-border-style-painted-variants
status: planned
depends-on: [native-engine-155]
---

# Native bounded painted physical border styles

## Objective

Complete the bounded physical border-style paint vocabulary with
case-insensitive `double`, `groove`, `ridge`, `inset`, and `outset`. The
`border-style` shorthand expands one to four values from the finite
`none|hidden|solid|dashed|dotted|double|groove|ridge|inset|outset` grammar into
physical top/right/bottom/left candidates; the four physical style longhands
own only their side. Standalone case-insensitive `revert-layer` remains
supported for all five style properties.

## Context

The native engine already owns deterministic solid, dashed, dotted, none, and
hidden physical border styles. The remaining painted CSS line styles need
behavior in both the public computed paint value and software replay. This
slice deliberately batches the five related values to avoid five independent
compile and certification cycles.

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-style>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-155.md`
- `docs/plan/tasks/native-engine-019.md`

## Contract

### Declaration and cascade state

- `border-style` accepts one to four values from the bounded
  `none|hidden|solid|dashed|dotted|double|groove|ridge|inset|outset` grammar
  and expands them using the physical top/right/bottom/left shorthand mapping.
  Each physical `border-*-style` longhand accepts one style value. All five
  property names accept one standalone, case-insensitive `revert-layer` token.
- Mixed tokens, other CSS-wide keywords, empty values, malformed one-to-four
  value expansions, unsupported styles, and unsupported CSS syntax remain
  typed unsupported-value diagnostics. Raw CSS text is not added to
  diagnostics or public protocol output.
- `double` paints two deterministic parallel stripes. For widths of at least
  three pixels, the outer and inner one-third bands paint with a one-third
  gap; narrower widths use a solid fallback so a valid border remains visible.
- `groove` and `ridge` paint two half-width deterministic shades of the
  declared color. `groove` darkens the outer half and lightens the inner half;
  `ridge` uses the inverse. `inset` darkens top/left and lightens bottom/right;
  `outset` uses the inverse edge shading. Alpha remains unchanged.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, same-block declaration order, and existing independent
  width/style/color resolution remain authoritative. Style `revert-layer`
  rolls back each physical side independently, including all five new painted
  values.

### Existing owners preserved

- `none` and `hidden` continue to block lower style candidates and resolve to
  the existing no-side/zero-width result. Width or color candidates cannot
  resurrect either no-paint style.
- `NativeBorderStyle` gains only the five bounded painted variants;
  `NativeBorderPaintSide`, the border display command, rounded masks, clipping,
  opacity, viewport projection, capture, point-hit, and semantic/source-order
  schemas remain structurally unchanged.
- Existing complete `border` and physical border shorthand declarations reuse
  the expanded painted style parser. Omitted-component `border:none`, CSS-wide
  reset forms, and collapsed-table conflict resolution remain outside this
  slice.

The slice remains fixture-relative, horizontal-tb, and bounded to physical
border styles. It does not add anti-aliased joins, percentage/fractional
widths, logical sides, `currentColor`, gradients, border-image, animation,
multiple origins, `!important` inversion, table layout/conflict resolution, or
browser-wide CSS border conformance.

## Tradeoffs

- Batching the five painted variants reduces repeated build overhead but
  increases the review and raster-regression surface of one checkpoint.
- Public enum variants make computed style truthful for paint consumers, while
  keeping the immutable display-list shape stable. The bounded shade model is
  deterministic and explicit rather than claiming browser color/fidelity
  parity.
- Narrow double borders use a solid fallback because sub-three-pixel integer
  geometry cannot represent two stripes and a gap without disappearing; this
  is a documented native approximation, not an implicit CSS conformance claim.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- parser acceptance and physical one-to-four expansion for all five painted
  values, existing `none`/`hidden`, physical longhands, standalone
  case-insensitive `revert-layer`, and typed rejection of malformed,
  CSS-wide, mixed, and unsupported inputs;
- public enum exposure without private-sentinel leakage, complete border
  shorthand reuse, named-layer/specificity/source-order/inline precedence,
  same-block order, repeated rollback, and valid-before-invalid preservation;
- double stripe geometry, groove/ridge shading, inset/outset edge direction,
  decoded raster, clipping, point-hit testing, capture, and semantic/source
  order; and
- focused `glass-browser` check and tests, full native integration and library
  tests, strict affected-package Clippy, rustdoc, paired-crate check/build,
  formatting, documentation, and final static gates. Remote CI remains
  unclaimed until an explicitly authorized push.

## Implementation

To be filled after the implementation checkpoint.

## Evidence

To be filled after local certification and bounded target cleanup.

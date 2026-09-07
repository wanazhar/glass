---
id: native-engine-153
scope: glass-browser/native-engine/cascade-border-style
status: planned
depends-on: [native-engine-152]
---

# Native bounded physical `border-style`

## Objective

Add standalone, case-insensitive `border-style`, `border-top-style`,
`border-right-style`, `border-bottom-style`, and `border-left-style` to the
existing physical border owner. The shorthand expands one to four bounded
`solid|dashed|dotted` values into independent physical style candidates;
longhands own only their side. Each property also accepts one standalone,
case-insensitive `revert-layer` token.

## Context

The native engine now has private per-side cascade streams for complete border
values, border colors, and border widths. Standalone width and color can
override their components without changing another component, but the only
available style still comes from a complete `border` or physical border
shorthand. This slice adds the final bounded style stream and composes all
resolved components at the existing `NativeBorder` owner.

Normative references:

- <https://www.w3.org/TR/css-backgrounds-3/#border-style>
- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthands>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-152.md`
- `docs/plan/tasks/native-engine-019.md`

## Contract

### Declaration and cascade state

- `border-style` accepts one to four values from the existing bounded
  `solid|dashed|dotted` grammar and expands them using the physical
  top/right/bottom/left shorthand mapping. Each physical `border-*-style`
  longhand accepts one existing style value. All five property names accept
  one standalone, case-insensitive `revert-layer` token.
- Mixed tokens, other CSS-wide keywords, empty values, malformed one-to-four
  value expansions, `none`, unsupported styles, and unsupported CSS syntax
  remain typed unsupported-value diagnostics. Raw CSS text is not added to
  diagnostics or public protocol output.
- Existing `border` and `border-top|right|bottom|left` declarations contribute
  their selected styles to the private style stream. Width-only and style-only
  declarations remain non-painting when no complete style-bearing border can
  be formed; standalone width and style declarations from any matching rules
  may combine with the existing color stream to form a bounded border.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, and same-block declaration order remain authoritative.
  Style `revert-layer` blocks only its current bounded layer, can roll back
  repeatedly, and falls back to no style when no lower style candidate
  remains. A no-style result is represented as no border side, preserving the
  existing zero-width/no-paint behavior without exposing a keyword.
- The resolved style stream is combined with the independently resolved width
  and color streams only after all components resolve. `revert-layer` never
  enters the public `NativeBorder` value.

### Existing owners preserved

- Resolved styles continue through the existing `NativeBorderSide`, outer and
  content box insets, physical border display-list command, rounded mask,
  ancestor clips, opacity groups, viewport projection, point-hit testing,
  capture dimensions, software raster, and semantic/source-order projection.
- Existing border width, color, side order, pattern phase, corner precedence,
  and zero-width/no-paint behavior remain unchanged. Standalone style values
  reuse the existing deterministic solid/dashed/dotted paint paths.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, and bounded to the current
physical solid/dashed/dotted style grammar. It does not add `none`, logical
sides, `currentColor`, gradients, border-image, double/groove/ridge/inset/
outset styles, animation, multiple origins, `!important` inversion, or
browser-wide CSS border conformance.

## Tradeoffs

- A third private style stream completes component ownership without replacing
  the existing border display and raster owners.
- Composing width, style, and color only after independent resolution allows
  declarations from separate bounded rules to form a border, while a lone
  width, style, or color declaration remains unable to invent missing paint
  components.
- No-style rollback returns no border side rather than inventing an unmodeled
  CSS `none` enum; this keeps the public finite style type and all zero-paint
  behavior explicit.
- The existing declaration-position encoding is reused for same-block order;
  no generic CSS cascade engine or extra dependency is introduced.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- one-to-four-value `border-style` expansion, physical style longhands,
  existing finite style grammar, case-insensitive standalone `revert-layer`,
  and typed rejection of mixed/CSS-wide/malformed/unsupported inputs;
- independent style versus width/color ownership, separate-rule composition,
  named-layer priority, specificity, source order, same-block order, repeated
  rollback, unlayered/inline precedence, valid-before-invalid preservation,
  and no-style/no-paint fallback;
- solid/dashed/dotted border geometry, display-list style, capture, decoded
  raster, clipping, point-hit testing, and semantic/source order; and
- no false unsupported diagnostics or public rollback leakage, plus focused
  `glass-browser` check, targeted behavioral tests, full native integration and
  library tests, strict affected-package Clippy, formatting, documentation,
  and final static gates. Remote CI remains unclaimed until an explicitly
  authorized push.

## Implementation

To be filled after the implementation checkpoint.

## Evidence

To be filled after local certification and bounded target cleanup.

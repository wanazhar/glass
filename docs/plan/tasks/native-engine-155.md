---
id: native-engine-155
scope: glass-browser/native-engine/cascade-border-style-hidden
status: planned
depends-on: [native-engine-154]
---

# Native bounded physical `border-style:hidden`

## Objective

Extend the bounded physical `border-style` surface with the explicit
case-insensitive `hidden` style. The `border-style` shorthand expands one to
four `none|hidden|solid|dashed|dotted` values into independent physical
candidates; the four physical style longhands own only their side. Each
property retains standalone case-insensitive `revert-layer` support.

## Context

The native engine now resolves explicit physical `none` through a private
no-paint sentinel. CSS defines `hidden` as no paint with the same zero used
border width as `none`, while reserving a distinction for collapsed-table
border conflict resolution. This slice keeps that distinction private even
though the current engine has no table-conflict owner.

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-style>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-154.md`
- `docs/plan/tasks/native-engine-019.md`

## Contract

### Declaration and cascade state

- `border-style` accepts one to four values from the bounded
  `none|hidden|solid|dashed|dotted` grammar and expands them using the
  physical top/right/bottom/left shorthand mapping. Each physical
  `border-*-style` longhand accepts one style value. All five property names
  accept one standalone, case-insensitive `revert-layer` token.
- Mixed tokens, other CSS-wide keywords, empty values, malformed one-to-four
  value expansions, unsupported styles, and unsupported CSS syntax remain
  typed unsupported-value diagnostics. Raw CSS text is not added to
  diagnostics or public protocol output.
- An explicitly resolved `hidden` style blocks lower style candidates and
  produces no `NativeBorder` side. In the current non-table engine it has the
  same no-side/zero-width result as `none`; the private distinction is retained
  for a future collapsed-table conflict owner. Width or color candidates cannot
  resurrect a side whose winning style is `hidden`; an isolated style-only
  declaration remains non-painting because it still has no width.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, and same-block declaration order remain authoritative.
  Style `revert-layer` can roll back past an explicit `hidden`, repeatedly when
  higher layers also roll back, and falls back to no style when no lower style
  candidate remains.
- The private `hidden` sentinel remains inside CSS cascade resolution. It is
  converted to the existing no-side/zero-width behavior before box-model,
  display-list, capture, raster, point-hit, and semantic/source-order owners.

### Existing owners preserved

- Existing bounded width/style/color component streams continue to resolve
  independently and compose only after all required components resolve.
- Existing `NativeBorderSide`, public `NativeBorderStyle`, public
  `NativeBorderPaintSide`, display-list command, rounded mask, clipping,
  opacity, viewport projection, capture, software raster, point-hit, and
  semantic/source-order schemas remain unchanged. No public `hidden` variant
  is added and no table-conflict semantics are claimed.
- Existing `border` and physical border shorthands retain the current
  complete-value grammar; `border: hidden`, omitted-component `border:none`,
  and other CSS-wide reset forms remain outside this slice.

The slice remains fixture-relative, horizontal-tb, and bounded to physical
`none|hidden|solid|dashed|dotted` styles. It does not add collapsed-table
border conflict resolution, `double`, `groove`, `ridge`, `inset`, `outset`,
logical sides, `currentColor`, gradients, border-image, animation, multiple
origins, `!important` inversion, or browser-wide CSS border conformance.

## Tradeoffs

- A distinct private hidden sentinel preserves future collapsed-table meaning
  without leaking a public style enum variant or changing current artifacts.
- Treating hidden as no-side/zero-width at the existing composition boundary
  matches current CSS used-value behavior and avoids inventing a table owner
  before the engine has table layout or conflict resolution.
- Supporting only the existing physical style stream keeps this slice small;
  complete border shorthand grammar and table behavior remain separately
  auditable contracts.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- one-to-four-value `none|hidden|solid|dashed|dotted` expansion, physical style
  longhands, case-insensitive standalone `revert-layer`, and typed rejection
  of mixed/CSS-wide/malformed/unsupported inputs;
- explicit hidden blocking lower styles, hidden rollback to lower paint,
  repeated rollback, named-layer priority, specificity, source order,
  same-block order, unlayered/inline precedence, valid-before-invalid
  preservation, and independent width/color interaction;
- no-side/zero-width geometry, absent border display commands, decoded raster,
  clipping, point-hit testing, capture, and semantic/source order; and
- no public hidden sentinel leakage or false unsupported diagnostics, plus
  focused `glass-browser` check, targeted behavioral tests, full native
  integration and library tests, strict affected-package Clippy, formatting,
  documentation, and final static gates. Remote CI remains unclaimed until an
  explicitly authorized push.

## Implementation

To be filled after the implementation checkpoint.

## Evidence

To be filled after local certification and bounded target cleanup.

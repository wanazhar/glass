---
id: native-engine-171
scope: glass-browser/native-engine/cascade-border-style-css-wide-keywords
status: planned
depends-on: [native-engine-170]
---

# Native bounded physical `border-style` CSS-wide keywords

## Objective

Accept the exact case-insensitive `inherit`, `unset`, `initial`, and `revert`
keywords for the existing local physical `border-style` owner: the
one-to-four-value `border-style` shorthand and the four physical style
longhands. Resolve them at the existing computed-style boundary while
preserving the finite supported style grammar, `revert-layer`, independent
side cascade, border width/color composition, content/outer geometry, and all
existing layout, display-list, capture, raster, clipping, opacity, scrolling,
point-hit, and semantic/source-order consumers.

This slice completes the bounded CSS-wide keyword family across the three
physical border component owners without introducing generic CSS value graphs,
logical sides, table conflict resolution, or browser-wide border conformance.

## Context

Native-engine-153 established the bounded physical border-style owner and its
private `Paint|None|Hidden` state; native-engine-122/145 established named
layers and lower-layer rollback; native-engine-169 and native-engine-170
established the private CSS-wide cascade pattern for physical border colors and
widths. The physical border-style parser still treats the four reset keywords
as invalid values.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#inheritance>
- <https://www.w3.org/TR/css-backgrounds-3/#border-style>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-170.md`
- `docs/plan/tasks/native-engine-153.md`
- `docs/plan/tasks/native-engine-145.md`

## Contract

### Declaration and resolution state

- `border-style` and each physical style longhand accept the existing finite
  `none|hidden|solid|dashed|dotted|double|groove|ridge|inset|outset` grammar,
  case-insensitive `revert-layer`, and one exact case-insensitive token each
  for `inherit`, `unset`, `initial`, and `revert`. A shorthand CSS-wide token
  expands to all four physical sides; mixed CSS-wide/style tokens remain
  unsupported.
- A private declaration value distinguishes a supported painted style, the
  existing private `none` and `hidden` sentinels, `inherit`, `unset`,
  `initial`, and one-author-origin `revert` until the winning side-local
  candidate is resolved. `revert-layer` remains the existing lower-layer
  sentinel. No declaration-only variant may leak into public computed-style
  fields, border commands, capture bytes, or raster data.
- The property remains local/non-inherited for ordinary omission: no style
  candidate resolves to the existing no-style/zero-width fallback. Explicit
  `inherit` copies the parent's effective concrete physical style, including a
  parent `none`/`hidden` result or a style that is not paintable because its
  width or color component is missing. A bounded root with no parent uses the
  existing no-style fallback.
- `unset`, `initial`, and one-author-origin `revert` resolve to the existing
  private `none` style. This preserves the bounded initial/no-paint result;
  style-only declarations still do not invent a border, and `hidden` remains
  distinct from `none` for the future table-conflict boundary.
- The computed-style walk carries only a private four-side effective style
  array. It must not turn an omitted child style into an inherited candidate.
  Only explicit `inherit` uses that array. Side order is physical
  top/right/bottom/left and remains independent through shorthand expansion,
  longhand precedence, layers, specificity, source order, and inline style.

### Existing owners preserved

- Border width/color resolution, no-paint sentinels, width/style/color
  composition, content/outer box geometry, display commands, border replay,
  clipping, opacity, scrolling, capture, software raster, point-hit, and
  semantic/source-order owners remain unchanged downstream.
- The transient parent style array is private to the style walk. Public
  computed-style fields, display-list commands, raster schemas, diagnostics
  transport, dependencies, feature defaults, and the two-crate boundary remain
  unchanged. `native-engine` remains default-off inside `glass-browser`.
- The slice remains local-resource-only, fixture-relative, horizontal-tb,
  integer-pixel, fixed-cell, non-table, and explicitly non-browser-parity.

## Tradeoffs

- One batched declaration family reuses the existing four-side cascade arrays
  and `revert-layer` resolver instead of adding a generic CSS-wide engine.
- Carrying four effective private style values makes explicit `inherit`
  observable without exposing parser sentinels or changing border artifacts.
- Reset forms intentionally resolve to the existing private `none` style,
  retaining the bounded initial and no-paint behavior. The `hidden` distinction
  remains available for future table conflict resolution instead of being
  collapsed into a public style enum.
- No dependency, public schema, layout algorithm, default feature, or crate
  boundary changes are allowed, keeping the build surface stable.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for all four keywords across shorthand
  and physical longhands alongside every supported concrete style and
  `revert-layer`, plus typed rejection of malformed, mixed, unsupported, and
  non-style values;
- explicit side-wise inherit from parent painted, `none`, `hidden`, unpainted,
  and root-fallback effective styles; reset/no-style behavior; ordinary
  omission; layer/unlayered/inline precedence; shorthand/longhand order; and
  valid-before-invalid preservation; and
- unchanged width/color composition and border geometry through border
  commands, decoded raster, clipping, opacity, capture, point-hit,
  semantic/source order, and private/public separation.

Focused feature check/tests, full native integration/library tests, strict
affected-package Clippy, rustdoc, paired-crate check/build, packaging,
formatting, documentation, final static gates, workspace all-target/
all-feature testing, and bounded regenerable-target cleanup are required.
Remote CI remains unclaimed until an explicitly authorized push.

## Implementation

Pending. This docs-first contract records the bounded physical border-style
CSS-wide keyword resolution before changing the parser, private inherited
side-style state, or computed-style resolver.

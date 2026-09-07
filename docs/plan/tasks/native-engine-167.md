---
id: native-engine-167
scope: glass-browser/native-engine/cascade-background-color-css-wide-keywords
status: planned
depends-on: [native-engine-166]
---

# Native bounded `background-color` CSS-wide keywords

## Objective

Accept the exact case-insensitive `inherit`, `unset`, `initial`, and `revert`
keywords for the existing local `background-color` property. Resolve them at
the existing computed-style boundary while preserving the private
`currentColor` path, public `Option<NativeColor>`, and every existing fill,
clipping, opacity, scrolling, capture, software-raster, point-hit, and
semantic/source-order consumer.

This slice extends the CSS-wide keyword work to one non-inherited paint
property. It does not introduce a generic CSS value graph, change the
inheritance of unrelated properties, or imply browser-wide background
conformance.

## Context

Native-engine-163 established bounded `background-color: currentColor`
substitution. Native-engine-145 established `background-color: revert-layer`
rollback through named-layer and unlayered candidate streams. Native-engine-166
completed the corresponding CSS-wide keyword family for inherited `color`, but
the local background owner still treats those keywords as unsupported.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#inheritance>
- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>
- <https://www.w3.org/TR/css-cascade-5/#cascade-origin>
- <https://www.w3.org/TR/css-backgrounds-3/#background-color>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-166.md`
- `docs/plan/tasks/native-engine-163.md`
- `docs/plan/tasks/native-engine-145.md`

## Contract

### Declaration and resolution state

- `background-color` accepts the existing bounded literal/alpha grammar,
  case-insensitive `currentColor`, case-insensitive `revert-layer`, and one
  exact case-insensitive token each for `inherit`, `unset`, `initial`, and
  `revert`.
- A private declaration value distinguishes `Color`, `CurrentColor`,
  `Inherit`, `Unset`, `Initial`, and `Revert` until the winning local
  declaration is resolved. None of these declaration-only variants may leak
  into `NativeComputedStyle`, display commands, capture bytes, or raster data.
- `background-color` is non-inherited. `inherit` resolves to the parent's
  computed optional background color; if that parent has no concrete fill, the
  child also has no fill. `unset`, `initial`, and one-author-origin `revert`
  resolve to the native initial no-fill result (`None`).
- An omitted local declaration retains the established no-fill behavior in
  direct stylesheet computation and layout. The layout helper's root walk
  carries the parent's optional background only as an explicit input for
  `inherit`; it must not turn ordinary omission into inheritance.
- `currentColor` continues to resolve from the element's already-computed
  local/inherited `color`, and `revert-layer` remains the separate lower-layer
  rollback operation. `revert` models only this engine's single author origin;
  it does not pretend to model user, user-agent, animation, or multiple-origin
  cascades.
- Named-layer priority, unlayered/inline precedence, specificity, source
  order, same-block order, valid-before-invalid preservation, and existing
  `!important` stripping remain unchanged. Malformed values, mixed tokens,
  other CSS-wide keywords, gradients, image functions, system colors,
  color-space functions, percentages, custom properties, and arbitrary
  functions remain typed unsupported-value diagnostics.

### Existing owners preserved

- The resolved concrete optional background color continues through fill
  display commands, descendant layout, clipping, opacity, viewport scrolling,
  capture, software raster, point-hit, and semantic/source-order owners
  without downstream keyword handling.
- The transient parent background input is private to the computed-style walk;
  public computed-style fields, display-list commands, raster schemas,
  diagnostics transport, dependencies, feature defaults, and the two-crate
  boundary remain unchanged. `native-engine` remains default-off inside
  `glass-browser`.
- The slice remains local-resource-only, fixture-relative, horizontal-tb,
  integer-pixel, fixed-cell, non-table, and explicitly non-browser-parity.

## Tradeoffs

- Batching all four keywords reuses the existing private declaration pattern
  while keeping the property-specific difference between inherited `color` and
  non-inherited `background-color` explicit.
- Carrying one optional parent background through `NativeInheritedStyle` is
  smaller than introducing a second style graph, and preserves `None` as the
  existing no-fill/public fallback. It also makes `inherit` distinguishable
  from ordinary omission and from `initial`.
- `unset` and `revert` intentionally resolve to no fill because this engine
  has one author origin and `background-color` is non-inherited. Modeling
  multiple origins or a broader transparent/background layer system requires a
  separate contract.
- No dependency, public schema, layout geometry, default feature, or crate
  boundary changes, so the build surface stays stable.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for all four keywords alongside literal,
  alpha, `currentColor`, and `revert-layer`, plus typed rejection of malformed,
  mixed, unrelated, and unsupported CSS values;
- child `inherit` copying a parent's concrete and omitted optional fill,
  explicit no-fill reset for `unset`/`initial`/`revert`, root fallback,
  ordinary omission, layer/unlayered/inline precedence, specificity,
  same-block order, rollback, and valid-before-invalid preservation;
- propagation through fill display commands, clipping, opacity, scrolling,
  capture, raster, point-hit, and semantic/source-order owners, with private /
  public separation; and
- focused feature check/tests, full native integration/library tests, strict
  affected-package Clippy, rustdoc, paired-crate check/build, packaging,
  formatting, documentation, final static gates, workspace all-target/
  all-feature testing, and bounded regenerable-target cleanup. Remote CI
  remains unclaimed until an explicitly authorized push.

## Implementation

Pending. This docs-first contract records the bounded non-inherited
`background-color` keyword resolution before changing the parser, inherited
style walk, or computed-style resolver.

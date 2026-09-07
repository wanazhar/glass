---
id: native-engine-172
scope: glass-browser/native-engine/cascade-border-radius-css-wide-keywords
status: planned
depends-on: [native-engine-171]
---

# Native bounded physical `border-radius` CSS-wide keywords

## Objective

Accept the exact case-insensitive `inherit`, `unset`, `initial`, and `revert`
keywords for the existing local physical `border-radius` shorthand. Resolve
them at the computed-style boundary while preserving bounded one-to-four-value
integer-pixel corner expansion, conservative normalization, `revert-layer`,
rounded layout/hit geometry, display-list replay, software raster, and PNG
capture.

This slice extends the bounded CSS-wide family to the remaining physical
border geometry owner. It does not add corner longhands, elliptical radii,
percentages, logical sides, or browser-wide border conformance.

## Context

Native-engine-020 established the physical `border-radius` shorthand and
rounded fill/border/hit consumers; native-engine-145 established the local
layer rollback pattern; native-engine-166 through native-engine-171 establish
the private CSS-wide declaration pattern for inherited and local owners. The
radius parser currently accepts concrete pixel values and `revert-layer` but
rejects the four reset keywords.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>
- <https://www.w3.org/TR/css-backgrounds-3/#border-radius>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-171.md`
- `docs/plan/tasks/native-engine-020.md`
- `docs/plan/tasks/native-engine-145.md`

## Contract

### Declaration and resolution state

- `border-radius` accepts the existing concrete one-to-four-value bounded
  non-negative integer `Npx` grammar, standalone case-insensitive
  `revert-layer`, and one exact case-insensitive token each for `inherit`,
  `unset`, `initial`, and `revert`. CSS-wide keywords cannot be mixed with
  concrete values or slash-separated radii.
- A private declaration value distinguishes concrete `NativeBorderRadius`,
  `inherit`, `unset`, `initial`, and one-author-origin `revert` until the local
  candidate resolves. `revert-layer` remains the existing lower-layer
  sentinel. No declaration-only state may leak into public radius values,
  display commands, capture bytes, raster data, or diagnostics transport.
- The property remains local/non-inherited for ordinary omission: omission
  resolves to `NativeBorderRadius::default()`. Explicit `inherit` copies the
  parent's effective concrete four-corner radius. A bounded root without a
  parent uses the default radius.
- `unset`, `initial`, and one-author-origin `revert` resolve to the default
  zero-corner radius. This preserves the current no-rounding fallback while
  keeping reset candidates distinct in the private cascade.
- The DOM style walk carries only one private effective parent radius. It must
  not turn an omitted child radius into an inherited candidate. Existing
  physical corner order remains top-left, top-right, bottom-right,
  bottom-left through normalization and all consumers.

### Existing owners preserved

- Concrete radius expansion, conservative corner normalization, rounded fill
  and border replay, clipping, opacity, scrolling, capture, software raster,
  point-hit, semantic/source order, and the public `NativeBorderRadius` shape
  remain the downstream owners.
- No dependency, public schema, layout algorithm, default feature, or crate
  boundary changes are allowed. `native-engine` remains default-off inside
  `glass-browser`; the workspace remains exactly two installable crates.
- The slice remains local-resource-only, fixture-relative, horizontal-tb,
  integer-pixel, fixed-cell, non-table, and explicitly non-browser-parity.

## Tradeoffs

- One private radius declaration wrapper reuses the existing bounded candidate
  array and `revert-layer` resolver instead of introducing generic CSS-wide
  value graphs.
- Carrying one effective parent radius is sufficient for explicit `inherit`
  without exposing parser sentinels or changing the public radius structure.
- Reset forms intentionally resolve to zero corners, preserving the existing
  bounded initial behavior. Elliptical and percentage geometry remain outside
  the engine's deterministic integer-pixel contract.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for all four keywords across the
  shorthand, concrete one-to-four-value expansion, standalone
  `revert-layer`, and typed rejection of mixed, slash-separated, fractional,
  percentage, negative, oversized, and unsupported values;
- explicit inherit from painted/rounded, default, and root-fallback parent
  radii; reset and ordinary omission; named-layer/unlayered/inline precedence;
  valid-before-invalid preservation; and deterministic corner order; and
- unchanged rounded layout, border/fill display commands, clipping, opacity,
  scrolling, capture, software raster, point-hit, semantic/source order, and
  public/private separation.

Focused feature check/tests, full native integration/library tests, strict
affected-package Clippy, rustdoc, paired-crate check/build, packaging,
formatting, documentation, final static gates, workspace all-target/
all-feature testing, and bounded regenerable-target cleanup are required.
Remote CI remains unclaimed until an explicitly authorized push.

## Implementation

Pending. This docs-first contract records the bounded physical `border-radius`
CSS-wide keyword resolution before changing the parser, private inherited
radius state, or computed-style resolver.

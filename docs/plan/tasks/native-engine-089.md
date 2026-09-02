---
id: native-engine-089
scope: glass-browser/native-engine/align-self-normal
status: active
depends-on: [native-engine-088]
---

# Native bounded align-self normal

## Objective

Extend the bounded non-inherited `align-self` grammar with the explicit
`normal` keyword for eligible direct flex items. In the supported row and
row-reverse flex context, an explicit `align-self:normal` must reuse the
completed stretch used-size and complete-artifact owners while remaining
distinct from omitted `align-self:auto` in computed style and cascade.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Box Alignment Level 3: flex self-alignment](https://drafts.csswg.org/css-align/)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the explicit, non-inherited item value
`align-self:normal` alongside the completed `auto`, `flex-start`, `center`,
`flex-end`, and `stretch` values. Parsing remains case-insensitive and accepts
one token only. Computed style retains `Normal` as a distinct value;
`align-self:auto` remains the only omitted/default value and continues to
resolve from the parent's computed `align-items` at the existing placement
boundary.

For an eligible direct element child of the existing fixed-width row or
row-reverse flex layout, explicit `align-self:normal` uses the same used-value
behavior as explicit `align-self:stretch`:

- an omitted `height` fills the existing line cross size minus vertical
  margins, never shrinking below the natural height from the shared child
  layout pass;
- physical padding and border insets, bounded pixel `min-height`, and bounded
  pixel `max-height` constrain the resulting outer box through the existing
  box-model owner;
- single-row explicit parent content height and wrapped formed line height
  after line-gap and `align-content` distribution remain authoritative;
- an explicit bounded `height` is preserved and uses the bounded flex-start
  placement fallback.

An explicit `align-self:normal` overrides every parent `align-items` value in
the supported context. It therefore stretches an auto-height item even when
the parent is `center`, `flex-end`, or the omitted native `flex-start`
fallback. In contrast, `align-self:auto` remains parent-controlled, and
explicit `align-self:flex-start|center|flex-end|stretch` retains each existing
behavior. A child value is not inherited from its parent; the parent's
`align-items` value remains its own computed property.

Cascade specificity, source order, inline precedence, non-inheritance, and
invalid-declaration retention remain unchanged. A valid `normal` declaration
wins at its existing cascade position; an invalid later declaration does not
erase an earlier valid item value. The value remains observable in computed
style and does not introduce a new diagnostic for supported flex use.

Outside the bounded row/row-reverse flex layout, this slice does not claim
general block, grid, absolute-positioned, writing-mode, logical-axis,
baseline, safe/unsafe, auto-margin, column-direction, or intrinsic-sizing
semantics for `normal`. CSS-wide keywords, multi-token forms, fractional,
percentage, and unsupported values remain typed diagnostics or out of scope.
No parent `align-items`, `align-content`, `place-content`, line-formation,
main-axis, or initial-value behavior changes.

The resulting layout box and complete descendant artifact range must remain
consistent across line overflow, display-list paint, software rasterization,
viewport projection, hit testing, scrolling, capture, and semantic/source
order. The slice reuses the existing stretch helper and introduces no second
cross-axis geometry representation.

## Tradeoffs

- A distinct computed `Normal` value preserves specified-value provenance and
  allows future layout-mode-specific behavior, at the cost of one explicit
  used-value mapping in the flex owner.
- Reusing the completed stretch path gives explicit `normal` and `stretch`
  one geometry/artifact owner. The bounded mapping intentionally does not
  pretend to implement `normal` for every CSS layout mode.
- Explicit heights remain fixed and top-aligned under the existing bounded
  fallback. This protects author-declared dimensions while declining full CSS
  used-value parity for all cross-size combinations.
- Physical integer-pixel padding, borders, and min/max dimensions remain the
  only supported constraints. Logical properties, fractional metrics, real
  font metrics, and writing modes remain outside the native boundary.
- No new dependency, crate, runtime, or artifact pipeline is added; the
  native backend remains default-off inside `glass-browser`, and Chromium/CDP
  remains the production runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Design is recorded in this task. Implementation, focused parser/cascade and
shared-layout tests, full native suites, strict feature/documentation gates,
issue #40 status, and exact regenerable-target cleanup will be recorded here
when the slice closes. Remote CI is not claimed until the local branch is
pushed.

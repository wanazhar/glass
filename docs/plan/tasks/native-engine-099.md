---
id: native-engine-099
scope: glass-browser/native-engine/flex-directionality
status: ready
depends-on: [native-engine-098]
---

# Native bounded Flexbox directionality

## Objective

Make inherited `direction:ltr|rtl` participate in the existing bounded
Flexbox axis mapping. Horizontal rows must start from the inline start of the
current direction, while columns keep their vertical main axis and reflect
their horizontal cross axis and wrapped line stacking. Preserve one layout and
artifact owner, source/semantic order, and the 098 line-local auto-margin
behavior.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-098.md`
- [CSS Flexible Box Layout Module Level 1: flex-direction](https://www.w3.org/TR/css-flexbox-1/#flex-direction-property)
- [CSS Flexible Box Layout Module Level 1: axis mappings](https://www.w3.org/TR/css-flexbox-1/#axis-mappings)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts only the bounded inherited values
`direction:ltr` and `direction:rtl`. The computed value is inherited through
the existing DOM style walk, defaults to `ltr`, participates in normal cascade
and inline-style precedence, and remains a typed computed state. Unsupported
CSS-wide, bidi-override, writing-mode, and malformed forms retain the existing
diagnostic/fallback behavior.

Layout consumes the direction state only in the existing eligible Flexbox
owners, under the current horizontal-tb and integer-pixel assumptions:

- for `flex-direction:row`, the physical main start is left for `ltr` and
  right for `rtl`; `row-reverse` swaps that mapping. Existing gaps, flex
  grow/shrink/basis, justify distribution, line-local auto margins, overflow,
  and integer remainder rules run in that logical main direction;
- for `flex-direction:column|column-reverse`, the vertical main axis and its
  reverse mapping are unchanged. `direction:rtl` changes the horizontal
  cross-start to the right, so `align-items`/`align-self` start/end and
  horizontal auto margins use the existing cross-axis owner with the reflected
  physical mapping;
- for wrapped columns, the horizontal line stacking direction is the XOR of
  `wrap-reverse` and inherited `rtl`. Wrapped rows retain their vertical
  cross-axis mapping because direction does not change the horizontal-tb block
  axis;
- direction combines with `row-reverse`, `column-reverse`, and `wrap-reverse`
  without sorting or reversing DOM children. Semantic, keyboard, and source
  order remain the document order, and every line/item subtree continues
  through the same layout, overflow, scroll, hit-test, display-list, raster,
  viewport, capture, and semantic consumers;
- nested flex owners inherit direction independently through the existing
  parent chain. A descendant may override the inherited value with normal
  selector or inline cascade precedence;
- non-flex normal flow, direct-text glyph order, Unicode bidi/shaping,
  `text-align:start|end`, logical properties, vertical writing modes, grid,
  floats, and browser-wide directionality remain outside this slice and retain
  their existing bounded fallback behavior.

## Tradeoffs

- Keeping the feature in the existing Flexbox owner makes RTL rows and column
  cross-axis placement useful without inventing a second coordinate system, but
  it deliberately does not claim general bidi or inline formatting behavior.
- Direction is inherited in computed style even where layout ignores it. This
  keeps the cascade state coherent for nested flex descendants while avoiding a
  false claim that the fixed-cell text rasterizer performs Unicode bidi.
- Direction changes physical start/end mapping, not source order. This follows
  the Flexbox accessibility model and avoids changing semantic references or
  keyboard traversal while visual placement changes.
- `rtl` is implemented only for the current horizontal-tb model. Vertical
  writing modes and logical physical-edge properties remain explicit future
  work rather than being approximated as horizontal RTL.
- The implementation combines existing boolean reverse flags at the owner
  boundary. This keeps wrapping, auto margins, and artifact translation
  line-local and deterministic, at the cost of leaving unsupported layout
  modes on their current fallback path.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation gate will include:

- CSS parser, cascade, inheritance, inline precedence, and unsupported-value
  diagnostics for `direction:ltr|rtl`;
- focused row and row-reverse tests for `ltr`/`rtl`, with and without wrapping,
  `wrap-reverse`, justify distribution, auto main margins, reverse physical
  placement, and source/semantic order preservation;
- focused column and column-reverse tests for `ltr`/`rtl`, with cross-axis
  alignment, horizontal auto margins, wrapped line stacking, `wrap-reverse`,
  `align-content`, and reverse physical placement;
- nested descendant geometry, display-list paint, software raster, viewport
  projection, capture, scroll, semantic, and point-hit consumers;
- explicit non-flex and text fallback tests proving that direction does not
  claim Unicode bidi or normal-flow text reordering;
- `cargo fmt --all -- --check`, focused and full native integration tests,
  feature-enabled library tests with the documented explicit stack, strict
  all-feature and no-default-feature Clippy, warning-denied rustdoc, locked
  binaries, package/dependency gates, fuzz checking, documentation/release
  validators, and exact isolated-target cleanup.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.

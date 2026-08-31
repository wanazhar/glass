---
id: native-engine-053
scope: glass-browser/native-engine/functional-alpha-colors
status: active
depends-on: [native-engine-052]
---

# Native bounded functional alpha colors

## Objective

Extend the existing bounded color grammar with a deterministic functional
alpha form so translucent backgrounds, borders, and text can use the same
`NativeColor` and software source-over owners already exercised by opacity
groups. This is a CSS input-surface slice, not a new paint or geometry system.

## Contract

The native CSS grammar accepts `rgba(R, G, B, A)` for `background-color`,
`color`, and the existing physical border declarations. `R`, `G`, and `B`
are decimal integer channels in the closed range `0` through `255`. `A` uses
the existing bounded fixed-point alpha grammar: a decimal value from `0` to
`1` with at most three fractional digits, or a percentage from `0%` to
`100%`. Alpha is quantized to 8-bit round-to-nearest using the same helper as
`opacity`.

Existing named colors, `transparent`, `#RGB`, `#RRGGBB`, `#RRGGBBAA`, and
three-channel `rgb(R, G, B)` remain supported without behavior changes.
Whitespace around function arguments is accepted; extra channels, CSS
modern-space syntax, channel percentages, signs, exponents, missing
arguments, out-of-range values, and malformed functions are unsupported and
produce the existing bounded CSS diagnostic without changing the cascade.

Parsed alpha colors flow through computed style into fill, border, and text
display commands and the existing integer source-over rasterizer. Layout,
line breaking, hit testing, clipping, scrolling, opacity-group boundaries,
capture, revisions, and action behavior do not change. No new dependency,
color space, premultiplied storage format, or browser color-management parity
is introduced.

## Tradeoffs

- Reusing the fixed-point opacity parser keeps alpha quantization consistent
  across functional colors and subtree opacity, but intentionally excludes
  the full CSS Color 4 grammar.
- Decimal integer channels are predictable and bounded, but channel
  percentages, modern `rgb()` slash syntax, wide-gamut colors, and color
  interpolation remain unsupported.
- Functional alpha makes transparent paint fixtures easier to express while
  retaining the existing source-over rounding and logical RGBA surface
  behavior; it does not imply screenshot or physical-pixel fidelity.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- `rgba` parsing covers valid channels, alpha decimals/percentages, case and
  whitespace handling, and invalid values;
- computed-style cascade and diagnostics cover functional alpha colors in
  background, text, and border declarations;
- a real display-list/raster integration fixture proves alpha background and
  text/border colors use the existing bounded source-over result;
- existing opacity-group, clipping, scrolling, capture, layout, hit-test,
  navigation, and revision behavior remains green;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

Implementation is not yet complete. This task records the contract before
source changes; the next checkpoint must replace this section with exact
local test, lint, documentation, commit, issue, and cleanup evidence.

---
id: native-engine-016
scope: glass-browser/native-engine/box-model-layout
status: done
depends-on: [native-engine-015]
---

# Native bounded box-model layout

## Objective

Make the existing border and text paint primitives consume an explicit,
bounded box-model layout:

- parse uniform non-negative `padding:Npx` and `margin:Npx` declarations;
- parse `box-sizing: content-box|border-box` with content-box as the bounded
  default;
- expose each layout box's outer border rectangle and derived content
  rectangle;
- make padding and border insets move child layout and direct text origins;
- make uniform margins participate in normal block/inline flow; and
- preserve deterministic point hit testing and paint order over the outer box.

This is a bounded geometry slice, not general CSS box sizing. At the 016
checkpoint it did not add negative/percentage/auto values, four-side
shorthands, margin collapsing,
min/max constraints, positioned/flex/grid layout, fractional metrics,
scrolling, transforms, or browser-parity claims.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/browser-host-rfc.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-014.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The existing bounded CSS parser accepts only a uniform non-negative pixel
value for `padding` and `margin`, and only `content-box` or `border-box` for
`box-sizing`. Unsupported or malformed values are ignored using the existing
declaration policy. Cascade precedence remains selector specificity, source
order, then inline precedence.

`NativeLayoutBox::rect` remains the outer border box used for hit testing,
backgrounds, borders, and clipping. Its new `content_rect` is the inner box
after the uniform border and padding insets. A specified width or height is a
content-box size by default; `border-box` treats it as the outer size. Auto
block width still fills the available outer width, while auto inline width is
the bounded intrinsic content width plus insets. Explicit uniform margins
reduce available inline/block width and advance normal flow by their outer
space. Margin collapsing and negative overflow are not modeled.

Direct text paint starts at `content_rect`, and descendant layout starts at
the same content origin and width. The document revision remains unchanged by
layout derivation; layout remains a Rust-only derived artifact.

## Tradeoffs

- A public content rectangle makes the geometry owner inspectable and keeps
  paint/hit-test responsibilities separate, but it expands the experimental
  Rust layout API and requires callers to distinguish outer/content boxes.
- Content-box defaults are closer to CSS semantics, but existing fixed-size
  border fixtures grow when borders are present; the change is intentional and
  is covered by updated geometry assertions.
- Uniform margins are useful for deterministic fixtures, but ignoring margin
  collapsing and four-side values at this checkpoint meant this was not a
  general CSS layout engine. The later 025 slice owns the bounded physical
  shorthand and longhand extension; unsupported values remain explicit by
  omission.
- No new dependency or stable backend field is introduced, preserving default
  build cost and transport compatibility.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/tests/native_engine.rs`
- native-engine design/plan/public capability documentation

## Verification

- `cargo fmt --all -- --check`
- focused native integration and CSS/layout unit tests;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` test matrix;
- documentation coverage/depth/release-truth validators;
- `git diff --check` and a clean focused Conventional Commit.

## Completion evidence

- Parser and cascade coverage: native CSS unit tests pass, including bounded
  padding/margin and `content-box`/`border-box` parsing and inline precedence.
- Geometry and paint coverage: native integration tests pass, including outer
  and content rectangles, border/padding insets, uniform margin flow,
  content-origin text paint, deterministic hit testing, and the revised border
  fixture.
- Focused tests: `24` native integration tests and `23` native unit tests pass.
- Feature lint: strict native-engine Clippy passes; default-feature check and
  strict Clippy also pass.
- Full locked matrix: `805` tests pass and `1` remains intentionally ignored
  across all glass-browser targets/features.
- Documentation coverage, depth, release-truth, formatting, and diff checks
  pass before the focused Conventional Commit and issue #40 update.

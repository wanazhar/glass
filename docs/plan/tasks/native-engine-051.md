---
id: native-engine-051
scope: glass-browser/native-engine/opacity-groups
status: done
depends-on: [native-engine-050]
---

# Native bounded opacity groups

## Objective

Add a bounded CSS `opacity` presentation slice to the existing native display
list and software rasterizer. An element with reduced opacity must composite
its own background, border, text, and rendered descendants as one subtree so
the result does not incorrectly apply the ancestor multiplier independently to
each child.

## Contract

The native CSS grammar accepts `opacity` as a non-negative alpha value in the
closed range `0` through `1`, or `0%` through `100%`, with at most three
fractional decimal digits. Values are quantized to an 8-bit alpha using
round-to-nearest; invalid, negative, over-range, exponent, and longer-fraction
forms are diagnosed as unsupported values and do not change the cascade.
Opacity is a local presentation property, not an inherited computed value.

Rendered elements with local opacity below `255` create bounded display-list
begin/end markers around their own box and all rendered descendants. The
software rasterizer replays those markers into a transparent off-screen layer,
then source-over composites the finished layer onto its parent using the group
alpha. Nested groups compose inside-out. `display:contents` elements may own a
group around their rendered descendants even though they have no layout box.

The group layer stack is bounded by a fixed depth and a total logical-pixel
budget derived from the existing native surface cap. Exceeding either bound
returns a typed native error before unbounded allocation. Unbalanced markers
are invalid display lists. `opacity:0` still participates in layout, semantic
visibility, point hit testing, scrolling, and actionability; it only removes
its painted output.

Clear commands remain the opaque root-surface initialization. Existing clips,
scroll translation, rounded masks, border patterns, PNG encoding, revision
checks, and display-list immutability remain owned by their current modules.
No general stacking context, transform, filter, blend mode, animation, or
browser compositing parity is claimed.

## Tradeoffs

- Off-screen subtree replay gives correct bounded group compositing, but it
  adds per-layer memory and a second pixel pass; the explicit depth and total
  pixel budget keep hostile fixtures from turning opacity into unbounded work.
- Opacity changes pixels only. Keeping zero-opacity boxes in layout and hit
  testing matches the useful CSS interaction behavior, but invisible controls
  can still receive semantic actions in this experimental engine.
- Fixed-point parsing avoids floating-point drift and extra dependencies, but
  values with more than three fractional digits are intentionally unsupported.
- The markers are internal display-list structure, so the public native API
  exposes the composited surface rather than promising a stable renderer
  command ABI.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs

## Verification

- opacity parsing, quantization, invalid-value diagnostics, local cascade, and
  non-inheritance are covered by unit tests;
- direct element paint, descendant paint, nested groups, `display:contents`,
  `opacity:0`, clipping, scrolling, and PNG replay are covered by focused
  integration tests;
- unbalanced markers, excessive group depth, and excessive aggregate layer
  pixels fail with bounded typed errors;
- the full native integration/unit suites, strict Clippy matrices, formatting,
  whitespace, and documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

Implementation completed locally. The focused native integration suite passed
65 tests and the native-engine module unit suite passed 49 tests, including
opacity parsing/cascade, nested and zero-opacity layout behavior, inside-out
software-layer compositing, and bounded marker/layer failures. Strict Clippy
passed with all features and with no default features, formatting and
whitespace checks passed, and the documentation/version/feature-parity
validators passed with zero current-claim failures. The checkpoint is ready
for its focused local Conventional Commit; remote CI remains pending because
this branch has not been pushed.

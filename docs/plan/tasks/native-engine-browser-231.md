# Native engine browser slice 231: relative positioning

Status: completed locally.

## Objective

Make the bounded native layout honor CSS `position: relative` and its
`top`/`right`/`bottom`/`left` offsets across computed style, layout, paint,
scroll projection, hit testing, and CSSOM-backed style mutation. Relative
positioning must move the rendered subtree without changing the space reserved
for the element or its siblings.

## Contract

- `position` accepts the static initial value and the relative positioning
  value. Other positioning modes remain diagnostic failures until their
  out-of-flow containing-block and stacking contracts are implemented.
- `top`, `right`, `bottom`, and `left` accept `auto`, unitless zero, and
  bounded signed integer pixel lengths. Values are capped by the native
  viewport-dimension limit before they enter computed style.
- When both edges on one axis are specified, the primary edge wins: `left`
  over `right` and `top` over `bottom`. If the primary edge is `auto`, the
  opposite edge supplies the inverse translation.
- Relative offsets translate the element's layout box and every descendant
  box/text run emitted during that layout call. The flow cursor still reserves
  the unshifted outer size, so following siblings retain their original flow
  position.
- The translated geometry is the single source for display-list paint,
  software capture, scroll projection, overflow clipping, and hit testing.
  No separate visual-only or input-only offset path is introduced.
- Stylesheet and inline declarations use the existing local cascade and
  `!important` ordering. CSS-wide reset keywords restore static/auto initial
  values, and CSSOM style mutation is observed on the next layout projection.

## Implementation

`css.rs` adds typed position and offset values, bounded signed-pixel parsing,
local cascade candidates, computed-style serialization, property diagnostics,
and CSSOM declaration support. `layout.rs` captures the subtree range emitted
for each element and applies one signed coordinate translation after its final
size is resolved. Existing paint-list, raster, scroll, and hit-test owners
consume the translated boxes without additional branching.

The implementation reuses the current fixed-size computed-style record and
content-worker transfer path; no dependency, crate, renderer, or process
boundary changed.

## Tradeoffs and follow-up

The slice deliberately uses integer pixel offsets and the existing unsigned
document coordinate space. A negative translation that reaches the document
origin is clamped there, matching the renderer's existing bounded-coordinate
policy. Percentage offsets, `position: absolute`/`fixed`/`sticky`, containing
blocks, out-of-flow layout, stacking contexts, `z-index`, and transform
composition remain separate work. They should extend the same typed geometry
and paint-order owners rather than bypassing flow or creating a second hit-test
model.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_relative_position -- --nocapture` (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_background -- --nocapture` (5 passed, 0 failed)

Implementation checkpoint: `882adc22`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.

---
id: native-engine-014
scope: glass-browser/native-engine/uniform-border-paint
status: done
depends-on: [native-engine-013]
---

# Native bounded uniform border paint

## Objective

Add the next renderer-owned primitive without changing layout ownership:

- parse a bounded uniform `border:Npx solid <color>` declaration;
- preserve the existing specificity, source-order, and inline cascade;
- emit one revisioned `BorderRect` command after a background and before direct
  text for each eligible layout box; and
- replay the border as an inside-the-box ring using the existing logical RGBA,
  source-over, surface-bound, and ancestor-clip rules.

This is a paint primitive, not a CSS box-model implementation. It does not add
padding, box sizing, individual border sides, non-solid styles, border-radius,
layout changes, scrolling, stacking contexts, transforms, screenshots, capture
transport, fonts, images, JavaScript, network access, or new dependencies.

## Context

- `docs/architecture/native-engine.md`
- `docs/browser-host-rfc.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-013.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The CSS presentation grammar accepts only the uniform shorthand
`border: <N>px solid <color>`. The width uses the existing bounded pixel
dimension parser, `solid` is the only accepted style keyword, and the color
uses the existing bounded named, hexadecimal, or `rgb(r,g,b)` grammar. Invalid
or unsupported border values are ignored. A zero-width border is a valid
computed value but emits no paint. Border declarations participate in the
same specificity, source-order, `!important` stripping, and inline precedence
as the other bounded presentation properties.

`NativeDisplayList::build` emits commands in layout-box order. For one box the
order is background fill, uniform border, then direct text. A border command
retains the document node, layout rectangle, width, color, and the same
half-open ancestor clip derived for fill/text. It is immutable inspection data
for the matching document revision; it never enters transport evidence or
becomes page state. A border is painted inside its layout rectangle so it does
not change the layout snapshot or point-hit ownership.

`NativeSurface` replays a border by blending pixels within the layout
rectangle that lie in the requested edge width from any side. It intersects
the rectangle with the logical surface and optional command clip before
iteration. Widths larger than a rectangle paint the bounded rectangle rather
than underflowing or allocating. Clear, fill, text, clipping, and source-over
behavior remain unchanged.

## Tradeoffs and what this misses

- One uniform solid shorthand is easy to audit and useful for cards/controls,
  but it misses individual sides, style variants, radius, joins, and box-model
  sizing.
- Painting inside the existing layout box keeps geometry and hit testing
  stable, but it is not CSS border-box/content-box behavior.
- A single `BorderRect` command keeps list bounds and ordering stable, but a
  future retained renderer may want side commands or a shared stroke primitive.
- Reusing ancestor clips and source-over blending preserves deterministic
  replay, but it does not model stacking contexts, masks, opacity groups, or
  antialiasing.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
cargo check -p glass-browser --no-default-features --locked
cargo clippy -p glass-browser --no-default-features --all-targets --locked -- -D warnings
RUST_MIN_STACK=8388608 cargo test -p glass-browser --all-targets --all-features --locked
```

The increased test-thread stack is required by the existing large CLI parser
test on the current ARM Linux host; the unmodified command aborts there before
any native failure is reported.

## Completion evidence

- `cargo fmt --all -- --check` passed.
- The native integration suite passed with 22 tests, including border parsing,
  deterministic command ordering, inside-the-box geometry, and clip-aware
  raster replay.
- The native-feature check and strict Clippy passed.
- The no-default-feature check and strict Clippy passed.
- `RUST_MIN_STACK=8388608 cargo test -p glass-browser --all-targets
  --all-features --locked` passed with 803 tests, 1 ignored, and all
  integration suites green.
- No dependency, transport capability, stable evidence, screenshot path, or
  two-crate boundary changed.

# Native-engine browser slice 531: three-point COLRv1 linear gradients

Status: complete on the current source line.

## Objective

Implement the three-point COLRv1 linear-gradient geometry described by the
[OpenType COLR specification](https://learn.microsoft.com/en-us/typography/opentype/spec/colr):
`p0` and `p1` define the color line and `p2` defines the direction in which
that color is projected across the glyph.

## Contract

- Carry finite `p0`, `p1`, and `p2` coordinates through the native gradient
  descriptor, affine paint transforms, synthetic horizontal stretch, and
  glyph-local pixel mapping.
- Compute the color-line parameter with the bounded cross-product projection
  `point = p0 + t(p1-p0) + u(p2-p0)`, then apply the admitted pad/repeat/
  reflect extension and the existing sorted stop interpolation.
- Reject collinear or otherwise degenerate point triples before color-layer
  admission; malformed gradients continue to use the monochrome fallback.
- Preserve the 16-stop, 32-layer, conformal-transform, clip-provenance,
  supersampled coverage, compositing, shaping, spacing, hit-testing, wire,
  two-crate, and explicit-CDP-backend contracts.

## Tradeoffs and remaining gates

The linear descriptor now honors the three-point projection, but arbitrary
gradient transforms, transformed clip masks, radial ellipse equations, sweep
skew parity, bitmap/SVG color fonts, palette selection and color variation
axes, hinting, font-display timing, and complete FontFace/Web IDL parity remain
explicit issue #40 work. The native implementation stays finite by rejecting
degenerate geometry rather than guessing a direction.

## Verification

- `cargo fmt --all` and `git diff --check` passed.
- `RUST_MIN_STACK=8388608 cargo check -p glass-browser --lib --locked` passed
  in 18.64 seconds.
- The linear-gradient sampler regression passed 1/1.
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib native_engine --locked -- --test-threads=1` passed 444/444 in 40.71 seconds.
- Release documentation passed with 1,181 Markdown documents, 83 current
  documents, 63 previous-version hits, 1,363 semantic audit hits, and zero
  current-claim failures.
- Documentation coverage passed with 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules; depth passed with 93
  routed/audited current guides and 19 substantive contracts; the TUI shortcut
  inventory passed with 15 implementation keys and 63 documentation markers.
- No remote CI, push, release, tag, registry publication, native/CDP parity
  certification, or issue closure is implied by this local checkpoint.

## Touched implementation

- `crates/glass-browser/src/browser/native_engine/font.rs`

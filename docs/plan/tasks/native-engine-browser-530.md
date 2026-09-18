# Native-engine browser slice 530: transformed COLRv1 gradients

Status: complete on the current source line.

## Objective

Expand the bounded COLRv1 gradient path through the paint transform stack so
common transformed linear gradients render in their transformed geometry and
radial/sweep gradients retain correct geometry for finite conformal transforms.

## Contract

- Apply the composed finite affine paint transform to linear gradient points
  before the compact descriptor reaches rasterization.
- Admit radial and sweep gradients through finite translation,
  rotation/uniform-scale, and reflection transforms only when the linear
  matrix is conformal; scale radii and map sweep angle direction accordingly.
- Reject singular matrices, skew/non-uniform radial or sweep transforms, and
  non-finite transformed coordinates before a color layer is admitted.
- Preserve the existing 16-stop, 32-layer, clip-provenance, supersampled
  coverage, compositing, shaping, spacing, hit-testing, wire-shape,
  two-crate, and explicit-CDP-backend contracts.
- Keep transformed clip boxes fail-closed while the current axis-aligned
  clip representation remains identity-transform-only; a malformed or
  unsupported transformed paint graph falls back to the monochrome outline.

## Tradeoffs and remaining gates

COLR linear gradients still use the existing bounded two-point projection and
validate, but do not yet apply, the third `x2/y2` linear-gradient point.
Radial gradients do not claim arbitrary affine ellipse support, sweep gradients
do not claim skewed-angle parity, and transformed clip boxes are not yet
represented as polygons. Full three-point gradient equations, arbitrary
gradient transforms, transformed clip masks, bitmap/SVG color fonts, palette
selection and color variation axes, hinting, font-display timing, and complete
FontFace/Web IDL parity remain explicit issue #40 work.

## Verification

- `cargo fmt --all` and `git diff --check` passed.
- `RUST_MIN_STACK=8388608 cargo check -p glass-browser --lib --locked` passed
  in 17.43 seconds.
- The conformal-transform geometry regression passed 1/1.
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib native_engine --locked -- --test-threads=1` passed 444/444 in 40.95 seconds.
- Release documentation passed with 1,180 Markdown documents, 83 current
  documents, 63 previous-version hits, 1,361 semantic audit hits, and zero
  current-claim failures.
- Documentation coverage passed with 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules; depth passed with 93
  routed/audited current guides and 19 substantive contracts; the TUI shortcut
  inventory passed with 15 implementation keys and 63 documentation markers.
- No remote CI, push, release, tag, registry publication, native/CDP parity
  certification, or issue closure is implied by this local checkpoint.

## Touched implementation

- `crates/glass-browser/src/browser/native_engine/font.rs`

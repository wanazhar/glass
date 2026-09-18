# Native-engine browser slice 529: bounded COLRv1 gradients

Status: complete on the current source line.

## Objective

Extend the bounded COLR/CPAL color-glyph path from solid paints to the common
COLRv1 linear, radial, and sweep gradient paints, while retaining bounded
clip-box coverage and a recoverable monochrome fallback for malformed or
unsupported color paint graphs.

## Contract

- Admit only finite, bounded gradient coordinates and at most 16 finite color
  stops whose offsets are in the inclusive `[0, 1]` interval.
- Preserve COLRv1 `pad`, `repeat`, and `reflect` extend modes and interpolate
  independent RGBA channels at raster sampling time.
- Carry a compact glyph-local gradient descriptor through rasterization; do
  not allocate a per-pixel gradient buffer. Map coordinates into the bounded
  glyph raster only after the outline and clip-box bounds are known.
- Apply the existing bounded `clipBox` masks during supersampled outline
  coverage, intersect nested boxes, and preserve the existing current-outline
  provenance guard.
- Keep gradient admission restricted to the currently supported identity paint
  transform. A transformed gradient, malformed stop list, unsupported paint,
  unsupported composite mode, or unbalanced painter state must fall back to
  the existing monochrome outline path rather than produce an incorrect color
  result.
- Preserve the 32-layer/16-level state bounds, native glyph wire shape,
  shaping, spacing, hit testing, software compositing, two-crate boundary,
  and explicit CDP migration backend.

## Tradeoffs and remaining gates

The raster sampler uses a bounded radial approximation for non-coincident
gradient centers and maps the current synthetic horizontal font stretch into
the gradient descriptor. This keeps the representation compact and finite but
is not complete COLRv1 gradient-transform or radial-equation parity. Gradient
transforms, all blend modes, bitmap color tables (CBDT/CBLC), SVG-in-font
sources, palette selection and color variation axes, hinting, font-display
timing, and complete FontFace/Web IDL parity remain explicit issue #40 work.

## Verification

- `cargo fmt --all` and `git diff --check` passed.
- `RUST_MIN_STACK=8388608 cargo check -p glass-browser --lib --locked` passed
  in 17.01 seconds.
- `cargo test --quiet -p glass-browser --lib colr_ --locked` passed 6/6.
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib native_engine --locked -- --test-threads=1` passed 443/443 in 41.21 seconds.
- Release documentation passed with 1,179 Markdown documents, 83 current
  documents, 63 previous-version hits, 1,359 semantic audit hits, and zero
  current-claim failures.
- Documentation coverage passed with 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules; depth passed with 93
  routed/audited current guides and 19 substantive contracts; the TUI shortcut
  inventory passed with 15 implementation keys and 63 documentation markers.
- No remote CI, push, release, tag, registry publication, native/CDP parity
  certification, or issue closure is implied by this local checkpoint.

## Touched implementation

- `crates/glass-browser/src/browser/native_engine/font.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`

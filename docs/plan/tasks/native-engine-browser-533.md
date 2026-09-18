# Native-engine browser slice 533: exact radial gradient equations

Status: complete on the current source line.

## Objective

Replace the native COLRv1 radial-gradient approximation with the two-circle
conical-gradient equation required by the OpenType COLR and HTML Canvas
algorithms, while preserving bounded software rasterization and the existing
native color-glyph ownership boundary.

## Contract

- Solve the finite two-circle geometry for the highest parameter whose circle
  has non-negative radius at each sampled pixel; retain the color-line extend
  mode and sorted stop interpolation.
- Handle concentric and non-concentric circles, equal radii, tangent points,
  nested circles, and the zero-radius cone tip without guessing a fallback
  color. Return no paint for degenerate or outside-cone samples.
- Keep finite coordinate/radius limits, conformal paint-transform admission,
  variation-coordinate stop selection, synthetic stretch, clipping, source-over
  compositing, shaping, and spacing unchanged.
- A gradient sample with no cone intersection is transparent rather than
  falling through to the text paint or monochrome glyph color.
- FontFace `local()` source resolution borrows the immutable system font book
  for each selected payload instead of cloning every installed face inside a
  JavaScript callback, keeping concurrent native runtimes bounded.
- Preserve the two-crate boundary and explicit CDP migration backend; this
  slice does not claim arbitrary affine radial ellipses, transformed clip masks,
  sweep skew parity, SVG-in-font, palette APIs, bitmap variation axes, hinting,
  font-display timing, or complete FontFace/Web IDL parity.

## Verification

- The exact radial-geometry regression passed 1/1.
- The transparent compositor-boundary regression passed 1/1.
- The existing gradient interpolation regression passed 1/1.
- The existing COLRv1 palette-gradient fixture regression passed 1/1.
- The native font unit group passed 38/38; the `script_font_face` group passed
  9/9 with two local-source tests running concurrently.
- The full locked `glass-browser` library regression passed 1,249 tests with
  1 ignored.
- `cargo check -p glass-browser --lib --locked`,
  `cargo check -p glass-dev --lib --bins --locked`, and
  `cargo build -p glass-dev --bin glass --locked` passed.
- Focused native integration gates for backend selection and real-font layout
  passed 1/1 each.
- The built native CLI navigated `about:blank`, observed the native page, and
  evaluated `1 + 1` without starting Chromium or a CDP socket.
- The native MCP stdio server completed `initialize` and `tools/list`; the
  native browser TUI rendered its native-engine ready state and exited cleanly
  on `Ctrl-C`.
- `cargo test --all-targets --locked` was started, but its 753-test
  `native_engine` integration target remained running beyond 18 minutes and
  was canceled; no all-target pass is claimed by this checkpoint.
- `cargo fmt --all -- --check` and locked metadata validation passed.
- No remote CI, push, release, tag, registry publication, native/CDP
  parity certification, or issue closure is implied by this local checkpoint.


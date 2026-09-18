# Native-engine browser slice 520: automatic variable font axes

Status: complete locally on the current source line.

## Objective

Make the inherited CSS `font-weight` and `font-stretch` controls select the
corresponding OpenType `wght` and `wdth` axes for variable faces, so ordinary
CSS typography does not depend on authors repeating low-level
`font-variation-settings` coordinates.

## Contract

- Inspect only axes advertised by the selected font face; static faces retain
  the existing path without synthetic variation settings.
- Map the bounded native weight values (`normal`/`bold`) to `wght` 400/700
  and CSS stretch tenths to `wdth` percentage values in OpenType coordinates.
- Preserve precedence: authored element coordinates override `@font-face`
  descriptor defaults, and either explicit coordinate suppresses its
  automatic `wght`/`wdth` insertion.
- Feed the effective coordinates to both HarfRust shaping and the variation
  outline rasterizer, retaining existing horizontal synthetic scaling for
  compatibility with descriptor ranges and non-variable fonts.
- Keep the existing bounded parser, face selection, resource wires, and
  fallback behavior unchanged for unsupported/unknown axes.

## Tradeoffs and explicit boundary

This closes the ordinary CSS-to-variable-axis path without adding a dependency
or changing the two-crate boundary. Synthetic scaling remains intentionally in
place, so a future axis-specific metric policy may refine double-scaling for
variable `wdth` faces. Numeric weight ranges, optical-size defaults, custom
axes, hinting, color tables, WOFF2, and complete FontFace/Web IDL parity remain
separate issue #40 gates.

## Verification

The final record will include a scoped package check, focused automatic-axis
tests, the affected `font_` group, the full locked `glass-browser` library
regression, and documentation inventory/depth/TUI gates. No remote CI, push,
release, tag, registry publication, or native/CDP parity certification is
implied by this local slice.

## Local results

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check --quiet -p glass-browser --lib --locked` passed in 12.93
  seconds after the automatic-axis implementation was applied.
- The focused variable-font witness passed `1/1` in 7.94 seconds; the
  affected `font_` regression passed `85/85` in 28.09 seconds.
- The full locked `glass-browser` library regression passed `1,220` tests with
  1 ignored and 0 failures in 62.31 seconds.
- The witness verifies that a variable face advertises and receives automatic
  `wght=700` and `wdth=75` coordinates when CSS weight/stretch are changed and
  no explicit axis overrides them. Explicit variation settings remain higher
  priority; synthetic scaling remains the documented compatibility baseline.

# Native engine browser slice 506: apply font-stretch to rendering

Status: completed locally on the current source line.

## Objective

Make the bounded `font-stretch` descriptor work reach the computed-style,
font-selection, measurement, and rasterization owners without changing the
two-installable-crate boundary.

## Contract

- Parse the single-value CSS `font-stretch` property as an inherited,
  CSS-wide-reset-aware percentage-tenths value while keeping the two-value
  form reserved for `@font-face` descriptor ranges.
- Carry the computed value through the private parent-style chain and expose
  it through the native `getComputedStyle()` surface as a canonical
  percentage.
- Select the closest face range after the existing weight/style match;
  retain tied unicode-ranged faces so per-codepoint coverage still chooses the
  correct face.
- Scale shaped advances, offsets, kerning, character-path advances, and
  glyph coverage horizontally using the requested stretch and the selected
  face's bounded nominal width.
- Preserve the existing font byte/parser limits, fallback behavior, content
  wire compatibility, and transactional FontFace admission contract.

## Implementation

`NativeComputedStyle` and `NativeInheritedStyle` now own a singleton
`NativeFontStretchRange` for the computed property. The cascade parser and
scratch state resolve it with the existing inherited declaration machinery.
`NativeTextMetrics` passes the requested percentage into `NativeFontBook`,
whose face score prioritizes weight/style and then range distance. Font runs
scale HarfRust advances and offsets before accumulating the pen, scale
character-path metrics and kerning, and resample glyph coverage into the
requested horizontal width. `local()` FontFace source lookup uses the
descriptor's nominal stretch, and JavaScript projects computed values as
percentage strings.

## Tradeoffs and explicit limits

The implementation stays deterministic and bounded by representing stretch
as integer percentage tenths and using integer fixed-point geometry. Singleton
face descriptors are treated as the source face's nominal width; a descriptor
range uses normal width as its synthetic baseline until a real variation-axis
owner exists. Horizontal coverage uses bounded nearest-sample resampling, so
it provides useful static-face synthesis without claiming the visual fidelity
of a variable-font `wdth` axis. Variable/color tables, WOFF2, font-display
timing, mixed-script shaping, cross-realm FontFace projection, and complete
FontFace/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_stretch -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked script_font_face -- --nocapture --test-threads=1`
- `git diff --check`

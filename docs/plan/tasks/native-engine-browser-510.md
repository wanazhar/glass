# Native engine browser slice 510: honor font-variant-caps during shaping

Status: completed locally on the current source line.

## Objective

Make the bounded inherited CSS `font-variant-caps` property affect native
computed style and HarfRust shaping by reusing the existing font-feature
ownership established by slices 507–509.

## Contract

- Parse `normal`, `small-caps`, `all-small-caps`, `petite-caps`,
  `all-petite-caps`, `unicase`, and `titling-caps`, together with the existing
  CSS-wide reset and malformed-value behavior.
- Carry the computed value through the private parent-style chain and expose
  it through native `getComputedStyle()` as a stable CSSOM value.
- Map the values to the bounded OpenType tags `smcp`, `c2sc` + `smcp`, `pcap`,
  `c2pc` + `pcap`, `unic`, and `titl` respectively.
- Let an authored `font-feature-settings` tag retain precedence over the
  corresponding convenience-property tag.
- Preserve existing font selection, fallback, byte limits, content-process
  wire compatibility, measurement, rasterization, and two-crate boundaries.

## Tradeoffs and explicit limits

The property controls OpenType capitalization only on the HarfRust shaping
path. Character-by-character fallback has no capitalization-feature owner and
keeps its existing bounded advances and glyph path. Real variable-font axes,
color glyph tables, WOFF2, font-display timing, mixed-script shaping, and
browser-wide CSS Fonts/Web IDL parity remain separate issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused `font_variant_caps` parser/cascade/CSSOM/shaping witnesses
- focused font and script FontFace regression groups
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib --locked -- --test-threads=1`
- repository documentation validators and `git diff --check`

The slice is complete only when implementation, focused behavior, affected
package regression, documentation, and local resource accounting agree. No
remote CI, push, release, tag, registry publication, or parity certification
is implied by this task.

## Results

- Four focused `font_variant_caps` parser, cascade, CSSOM, and OpenType-tag
  witnesses passed.
- The broader font regression group passed `55/55`.
- The locked affected-library regression passed with `1,190` tests passed and
  one ignored under `RUST_MIN_STACK=8388608`, completing in 67.00 seconds.
- Formatting and diff checks passed.
- Release-documentation truth passed for 1,160 Markdown documents with zero
  current-claim failures; documentation depth, TUI shortcut inventory, and
  documentation coverage also passed.
- The checkout remains local-only: no remote CI, push, release, tag, registry
  publication, or parity certification is claimed. The 19.3 G of exact
  regenerable temp build trees was reclaimed while the 36 G repository target
  cache was retained for compile speed.

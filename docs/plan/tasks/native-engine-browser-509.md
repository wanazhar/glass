# Native engine browser slice 509: honor font-kerning during shaping

Status: completed locally on the current source line.

## Objective

Make the bounded inherited CSS `font-kerning` property affect native computed
style and HarfRust shaping by reusing the slice 508 OpenType feature owner.

## Contract

- Parse the inherited `auto`, `normal`, and `none` values with the existing
  CSS-wide reset and malformed-value behavior.
- Carry the computed value through the private parent-style chain and expose
  it through native `getComputedStyle()` as a stable CSSOM value.
- Map `normal` to an explicit `kern=1` feature and `none` to `kern=0` while
  leaving `auto` to HarfRust's normal font behavior.
- Let an authored `font-feature-settings` `kern` tag override the
  `font-kerning` convenience property.
- Preserve the existing font selection, fallback, byte limits, content-process
  wire compatibility, measurement, rasterization, and two-crate boundary.

## Tradeoffs and explicit limits

The property controls the global kerning feature only on the HarfRust shaping
path; character-by-character fallback has no pair-positioning owner and keeps
its existing bounded advances. The `auto` value intentionally delegates to
the shaper, and explicit low-level feature settings win for deterministic
overrides. Language/script shaping, variable axes, color tables, WOFF2,
font-display timing, mixed-script runs, and browser-wide CSS Fonts parity
remain separate issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused `font_kerning` parser/cascade/computed-style witnesses
- focused kerning/font shaping and script-font regression groups
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib --locked -- --test-threads=1`
- repository documentation validators and `git diff --check`

The slice is complete only when the implementation, focused behavior,
affected package regression, documentation, and local resource accounting all
agree. No remote CI, push, release, tag, registry publication, or parity
certification is implied by this task.

## Results

- Four parser/cascade/CSSOM/shaping witnesses passed under the focused
  `font_kerning` filter.
- The broader font regression group passed `55/55`, including the shared
  HarfRust feature path, and the script FontFace group passed `8/8`.
- The locked affected-library regression passed under
  `RUST_MIN_STACK=8388608` with one test thread; the repository's existing
  default-stack CLI test constraint remains recorded rather than hidden.
- Formatting and the repository documentation truth/depth/TUI/coverage gates
  passed; the checkout remains local-only with no remote CI, push, release,
  tag, registry publication, or parity certification claim.

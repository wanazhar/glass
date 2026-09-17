# Native engine browser slice 507: honor font-variant-ligatures during shaping

Status: completed locally on the current source line.

## Objective

Make the bounded CSS `font-variant-ligatures` property affect the native
computed style and HarfRust shaping path without changing the two-installable-
crate boundary.

## Contract

- Parse `normal`, `none`, and one value from each of the common,
  discretionary, historical, and contextual ligature groups.
- Treat the property as inherited and CSS-wide-reset-aware, preserving the
  existing cascade precedence, malformed-value behavior, and bounded style
  scratch storage.
- Carry the computed value through the private parent-style chain and expose
  it through native `getComputedStyle()` as a stable canonical string.
- Translate the four groups into bounded HarfRust OpenType feature settings:
  `liga`/`clig`, `dlig`, `hlig`, and `calt`.
- Preserve all existing font selection, fallback, byte limits, rasterization,
  content-process wire compatibility, and transactional FontFace admission
  behavior.

## Implementation

`NativeFontVariantLigatures` stores the four inherited feature-group switches,
with CSS's common/contextual initial state. The CSS declaration parser,
cascade scratch, computed style, and document parent-style chain now carry the
value. Native text metrics pass it into the shaping owner, which supplies one
explicit HarfRust feature per OpenType tag before measuring or rasterizing a
run. The JavaScript computed-style projection serializes the normalized value
and includes the property in the enumerable CSSOM inventory.

## Tradeoffs and explicit limits

The implementation deliberately covers the interoperable ligature subproperty
without pretending to implement the whole `font-variant` family. Caps,
numeric, east-asian, position, `font-feature-settings`, language overrides,
variable-font axes, color glyphs, and mixed-script run shaping remain separate
issue #40 gates. Explicit feature settings are created only for the five
bounded ligature tags; required ligatures and unrelated OpenType features keep
the shaper's normal behavior. Unsupported values fail closed and fall back to
the existing inherited value.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_variant -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked script_font_face -- --nocapture --test-threads=1`
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib --locked -- --test-threads=1`
- `git diff --check`

The default-stack full-library command reaches the existing CLI argument test
but aborts on that test's stack overflow; the isolated test passes with the
explicit 8 MiB test-thread stack, and the full library run then passes 1,179
tests with one existing ignored test. This is a test-harness stack setting,
not a failure in the native font change.

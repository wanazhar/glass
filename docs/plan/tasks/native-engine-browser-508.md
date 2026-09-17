# Native engine browser slice 508: honor font-feature-settings during shaping

Status: completed locally on the current source line.

## Objective

Make the bounded CSS `font-feature-settings` property affect native computed
style and HarfRust shaping without changing the two-installable-crate
boundary.

## Contract

- Parse `normal` or a bounded comma-separated list of quoted four-byte
  OpenType tags with optional non-negative integer, `on`, or `off` values.
- Treat the property as inherited and CSS-wide-reset-aware, preserving the
  existing cascade precedence, malformed-value behavior, and heap-owned style
  scratch boundary.
- Carry the computed value through the private parent-style chain and expose
  it through native `getComputedStyle()` as a stable canonical string.
- Translate admitted tags to typed HarfRust `Feature` values during text
  measurement and rasterization.
- Apply explicit feature settings after the native `font-variant-ligatures`
  defaults so an authored `liga`, `clig`, `dlig`, `hlig`, or `calt` setting is
  the effective value for that tag.
- Bound the stored list to a fixed number of entries and reject an entire
  malformed or over-limit declaration without retaining raw stylesheet text.

## Implementation boundary

The CSS owner stores fixed-size tag/value records. The document owner carries
the computed value through its existing inherited-style chain. The font owner
converts valid tags to HarfRust features, removes earlier duplicate tags so
the last authored value wins, and keeps the existing fallback, face selection,
font-byte limits, measurement, rasterization, and content-process wire paths.
The JavaScript owner adds the enumerable CSSOM property and canonical
serialization.

## Tradeoffs and explicit limits

The fixed cap keeps deep-style computation stack-safe and prevents arbitrary
stylesheet data from becoming shaping state. The parser accepts only the
portable four-byte quoted-tag form; CSS escapes, unquoted tags, negative or
unbounded values, feature-range syntax, variable-font axes, language
overrides, and browser-specific feature discovery remain out of contract.
Explicit settings override the five bounded ligature tags, while HarfRust's
normal required-feature and script shaping behavior remains intact. This slice
does not claim complete `font-variant`, variable/color font, WOFF2,
font-display, mixed-script, FontFace/Web IDL, or browser-wide CSS Fonts parity.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused `font_feature_settings` parser/cascade/computed-style witnesses
- focused font shaping regression group
- serial script FontFace and existing font groups
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib --locked -- --test-threads=1`
- repository documentation validators and `git diff --check`

The slice is complete only when the implementation, focused behavior,
affected package regression, documentation, and local resource accounting all
agree. No remote CI, push, release, tag, registry publication, or parity
certification is implied by this task.

## Results

- Three parser/cascade/CSSOM witnesses passed under the focused
  `font_feature_settings` filter.
- The explicit `liga=0` shaping override witness passed; the shared font group
  passed `51/51` and the script FontFace group passed `8/8`.
- The locked affected-library regression passed `1,182` tests with one ignored
  under `RUST_MIN_STACK=8388608` and one test thread.
- Formatting and the repository documentation truth/depth/TUI/coverage gates
  passed; the checkout remains local-only with no remote CI, push, release,
  tag, registry publication, or parity certification claim.

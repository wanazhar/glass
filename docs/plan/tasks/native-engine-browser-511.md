# Native engine browser slice 511: honor font-variant-position during shaping

Status: completed locally on the current source line.

## Objective

Make the bounded inherited CSS `font-variant-position` property affect native
computed style and HarfRust shaping by reusing the existing OpenType feature
owner established by slices 507–510.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-510.md`

## Contract

- Parse `normal`, `sub`, and `super` with the existing CSS-wide reset and
  malformed-value behavior.
- Treat the property as inherited and carry the computed value through the
  private parent-style chain.
- Expose the value through native `getComputedStyle()` and the enumerable
  CSSOM property inventory using canonical `normal`, `sub`, or `super` text.
- Map `sub` to the bounded OpenType `subs` feature and `super` to `sups` on
  the HarfRust shaping path.
- Let an authored `font-feature-settings` `subs` or `sups` tag retain
  precedence over the convenience property.
- Preserve font selection, fallback, font limits, measurement, rasterization,
  content-process compatibility, and the two-installable-crate boundary.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/font.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`

## Tradeoffs and explicit limits

The property controls OpenType substitution only on the HarfRust shaping path.
Character-by-character fallback remains unchanged because it has no bounded
feature-application owner. This slice does not claim baseline shifting,
typographic superscript/subscript metrics, numeric variants, language or
script-specific feature policy, variable/color fonts, WOFF2, or complete CSS
Fonts/Web IDL parity.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused `font_variant_position` parser/cascade/CSSOM/shaping witnesses
- focused font regression group
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib --locked -- --test-threads=1`
- repository documentation validators and `git diff --check`

The slice is complete only when implementation, focused behavior, affected
package regression, documentation, and local resource accounting agree. No
remote CI, push, release, tag, registry publication, or parity certification
is implied by this task.

## Results

- Five focused parser/cascade/CSSOM/OpenType-precedence witnesses passed under
  the `font_variant_position` filter.
- The broader font regression group passed `59/59`.
- The locked affected-library regression passed `1,195` tests with one ignored
  under `RUST_MIN_STACK=8388608` and one test thread in 66.52 seconds.
- Formatting, diff checks, and the documentation validators passed locally.
- The checkout remains local-only: no remote CI, push, release, tag, registry
  publication, or native/CDP parity certification is claimed.

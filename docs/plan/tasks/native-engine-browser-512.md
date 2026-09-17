# Native engine browser slice 512: honor font-variant-numeric during shaping

Status: completed locally on the current source line.

## Objective

Make the bounded inherited CSS `font-variant-numeric` property affect native
computed style and HarfRust shaping by reusing the existing OpenType feature
owner established by slices 507–511.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-511.md`

## Contract

- Parse `normal`, one value from each of `lining-nums`/`oldstyle-nums`,
  `proportional-nums`/`tabular-nums`, and `diagonal-fractions`/
  `stacked-fractions`, plus `ordinal` and `slashed-zero`.
- Reject duplicate group values, `normal` combinations, unknown tokens, and
  malformed declarations while preserving the existing inherited CSS-wide
  reset behavior.
- Carry the computed value through the private parent-style chain and expose
  a stable canonical CSSOM value in the enumerable property inventory.
- Map the values to bounded OpenType tags `lnum`, `onum`, `pnum`, `tnum`,
  `frac`, `afrc`, `ordn`, and `zero`.
- Let authored `font-feature-settings` tags retain precedence for every
  convenience-property tag.
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

The property controls OpenType numeric substitution only on the HarfRust
shaping path. Character-by-character fallback remains unchanged because it has
no bounded feature-application owner. This slice does not claim numeric
feature-specific shaping algorithms, baseline/advance normalization,
language-specific behavior, variable/color fonts, WOFF2, or complete CSS
Fonts/Web IDL parity.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused `font_variant_numeric` parser/cascade/CSSOM/shaping witnesses
- focused font regression group
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib --locked -- --test-threads=1`
- repository documentation validators and `git diff --check`

The slice is complete only when implementation, focused behavior, affected
package regression, documentation, and local resource accounting agree. No
remote CI, push, release, tag, registry publication, or parity certification
is implied by this task.

## Results

- Four focused parser/cascade/CSSOM/OpenType-precedence witnesses passed under
  the `font_variant_numeric` filter.
- The broader font regression group passed `64/64`.
- The locked affected-library regression passed `1,199` tests with one ignored
  under `RUST_MIN_STACK=8388608` and one test thread in 61.85 seconds.
- Formatting, diff checks, and the documentation validators passed locally.
- The checkout remains local-only: no remote CI, push, release, tag, registry
  publication, or native/CDP parity certification is claimed.

# Native engine browser slice 513: honor font-variant-alternates

Status: completed locally on the current source line.

## Objective

Make the bounded inherited CSS `font-variant-alternates` property affect native
computed style, CSSOM, and OpenType shaping by reusing the existing native font
feature owner.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-512.md`

## Contract

- Parse `normal` and `historical-forms`, including the existing inherited
  CSS-wide reset keywords.
- Reject parameterized alternates and malformed or compound values rather than
  admitting an unimplemented feature-value mapping.
- Carry the computed value through the private parent-style chain and expose a
  stable canonical CSSOM value in the enumerable property inventory.
- Map `historical-forms` to the bounded OpenType `hist` feature.
- Let authored `font-feature-settings: "hist" ...` retain precedence over the
  convenience property.
- Preserve font selection, fallback, measurement, rasterization,
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

The slice implements only the keyword alternate whose OpenType mapping is
unambiguous without an `@font-feature-values` registry. `stylistic()`,
`styleset()`, `character-variant()`, `swash()`, `ornaments()`, and
`annotation()` remain rejected until the native registry and feature-value
resolution contract exist. The mapping affects HarfRust shaping only;
character-by-character fallback, variable/color tables, WOFF2, and complete
CSS Fonts/Web IDL parity remain separate issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused `font_variant_alternates` parser/cascade/CSSOM/OpenType-precedence
  witnesses
- focused font regression group
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib --locked -- --test-threads=1`
- repository documentation validators and `git diff --check`

The slice is complete only when implementation, focused behavior, affected
package regression, documentation, and local resource accounting agree. No
remote CI, push, release, tag, registry publication, or native/CDP parity
certification is implied by this task.

## Results

- Four focused parser, inheritance, CSSOM, and OpenType-precedence witnesses
  passed under the `font_variant_alternates` filter.
- The broader font regression group passed `68/68`.
- The locked affected-library regression passed `1,203` tests with one ignored
  under `RUST_MIN_STACK=8388608` and one test thread in 61.31 seconds.
- Formatting, diff checks, and the repository documentation validators passed
  locally.
- The checkout remains local-only: no remote CI, push, release, tag, registry
  publication, or native/CDP parity certification is claimed.

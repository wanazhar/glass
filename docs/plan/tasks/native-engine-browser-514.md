# Native engine browser slice 514: honor font-variant-east-asian

Status: completed locally on the current source line.

## Objective

Make the bounded inherited CSS `font-variant-east-asian` property affect native
computed style, CSSOM, and OpenType shaping by reusing the existing native font
feature owner.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-513.md`

## Contract

- Parse `normal`, one form value from `jis78`, `jis83`, `jis90`, `jis04`,
  `simplified`, and `traditional`; one width value from `full-width` and
  `proportional-width`; and the independent `ruby` flag.
- Reject duplicate form/width/ruby values, `normal` combinations, unknown
  tokens, and malformed declarations while preserving inherited CSS-wide reset
  behavior.
- Carry the computed value through the private parent-style chain and expose a
  stable canonical CSSOM value in the enumerable property inventory.
- Map form values to `jp78`, `jp83`, `jp90`, `jp04`, `smpl`, and `trad`; width
  values to `fwid` and `pwid`; and `ruby` to `ruby`.
- Let authored `font-feature-settings` tags retain precedence for every mapped
  convenience-property tag.
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

This slice maps the standard east-Asian keyword controls to OpenType feature
tags but does not synthesize glyphs when a selected font lacks a feature.
Character-by-character fallback remains bounded and does not apply feature
substitution. Language-specific shaping, vertical writing, variable/color
tables, WOFF2, and complete CSS Fonts/Web IDL parity remain separate issue #40
gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused `font_variant_east_asian` parser/cascade/CSSOM/OpenType-precedence
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
  passed under the `font_variant_east_asian` filter.
- The broader font regression group passed `72/72`.
- The locked affected-library regression passed `1,207` tests with one ignored
  under `RUST_MIN_STACK=8388608` and one test thread in 65.16 seconds.
- Formatting, diff checks, and the repository documentation validators passed
  locally.
- The checkout remains local-only: no remote CI, push, release, tag, registry
  publication, or native/CDP parity certification is claimed.

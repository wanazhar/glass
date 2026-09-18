# Native engine browser slice 517: apply bounded font variations

Status: complete locally on the current source line.

## Objective

Implement the inherited CSS `font-variation-settings` path for variable-font
shaping. Authored axis settings must survive native cascade and CSSOM and be
applied to HarfRust's `ShaperInstance` for selected faces.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-516.md`

## Contract

- Accept `normal` or a bounded comma-separated list of quoted four-byte
  printable OpenType axis tags and signed decimal axis values; normalize
  duplicate tags last-wins and reject malformed, escaped, empty, or
  out-of-bound entries atomically.
- Preserve inherited CSS-wide reset semantics for `inherit`, `initial`,
  `unset`, `revert`, and `revert-layer`, normal declaration order, and
  `!important` behavior.
- Carry the fixed-size representation through computed native styles and
  expose a deterministic canonical CSSOM value.
- Construct a HarfRust `ShaperInstance` from the selected axis coordinates
  before shaping, while retaining existing feature/language/direction and
  face-selection ownership.
- Preserve content-process defaults, existing fallback behavior, resource
  bounds, and the two-installable-crate boundary.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/font.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`

## Tradeoffs and explicit limits

The axis list is deliberately fixed-size and uses deterministic thousandths
within a finite numeric range rather than accepting unbounded CSS numbers or
escape syntax. HarfRust shaping consumes the selected coordinates; the
fontdue rasterizer and character-by-character fallback still use their
existing default-instance path because they have no variable-axis owner in
this slice. `@font-face`/script-created `variationSettings` descriptor
admission, variable glyph rasterization, color tables, and complete CSS Fonts
and Web IDL parity remain separate issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused `font_variation_settings` parser, cascade, CSSOM, and HarfRust
  witnesses
- focused `font_` regression group
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib --locked -- --test-threads=1`
- repository documentation validators and `git diff --check`

The slice is complete only when implementation, focused behavior, affected
package regression, documentation, and local resource accounting agree. No
remote CI, push, release, tag, registry publication, or native/CDP parity
certification is implied by this task.

## Results

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check --quiet -p glass-browser --lib --locked` passed in 15.27
  seconds after the final source and test corrections.
- The focused `font_variation_settings` parser, cascade, CSSOM, and HarfRust
  witness suite passed 4/4 tests in 39.48 seconds.
- The affected `font_` regression group passed 83/83 tests in 29.68 seconds.
- The full locked `glass-browser` library gate passed 1,218 tests with 1
  ignored in 64.43 seconds (99.25 seconds wall including the rebuild).
- Release-documentation truth passed over 1,167 Markdown documents with 83
  current documents, 63 previous-version hits, 1,341 classified semantic
  hits, and 0 current-claim failures. Documentation depth passed with 93
  routed current guides and 19 substantive contracts; the TUI inventory
  passed with 15 implementation help keys and 63 documentation markers; and
  documentation coverage passed with 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules.
- The checkout remains local-only: no remote CI, push, release, tag, registry
  publication, or native/CDP parity certification is claimed.

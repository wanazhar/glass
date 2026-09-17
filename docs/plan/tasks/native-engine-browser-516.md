# Native engine browser slice 516: honor `font-language-override`

Status: complete locally on the current source line.

## Objective

Carry the bounded inherited CSS `font-language-override` property through the
native cascade, computed CSSOM, and text-shaping path so an authored OpenType
language-system tag reaches HarfRust for the selected face.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-515.md`

## Contract

- Accept `normal` or one quoted four-byte printable OpenType tag, allowing
  only trailing space padding; reject unquoted, empty, internally padded,
  escaped, and otherwise malformed values without changing the declaration
  state.
- Preserve inherited CSS-wide reset semantics for `inherit`, `initial`,
  `unset`, `revert`, and `revert-layer`, with ordinary cascade order and
  importance intact.
- Carry the value through inherited and computed native styles, default
  missing serialized fields to `normal` for content-process compatibility, and
  expose an exact quoted canonical value through `getComputedStyle()`.
- Normalize an admitted padded tag to its unpadded HarfRust language value
  before shaping selected font faces; leave character-by-character fallback
  and existing feature-setting precedence bounded and unchanged.
- Preserve font selection, fallback, measurement, rasterization, script
  projection, document snapshots, and the two-installable-crate boundary.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/font.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`

## Tradeoffs and explicit limits

The parser intentionally accepts the fixed four-byte OpenType language-system
representation rather than unbounded CSS strings, BCP 47 negotiation, or
escape syntax. Only the HarfRust shaping path consumes the language override;
the bounded character fallback path has no language-system selector. This
slice does not provide full language negotiation, mixed-script or
vertical-writing parity, variable/color fonts, WOFF2, font-display timing,
cross-realm FontFace projection, or complete CSS Fonts/Web IDL parity; those
remain issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused `font_language_override` parser, cascade, CSSOM, and HarfRust
  witnesses
- focused `font_` regression group
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib --locked -- --test-threads=1`
- repository documentation validators and `git diff --check`

The slice is complete only when implementation, focused behavior, affected
package regression, documentation, and local resource accounting agree. No
remote CI, push, release, tag, registry publication, or native/CDP parity
certification is implied by this task.

## Results

- `cargo fmt --all -- --check` passed.
- `cargo check --quiet -p glass-browser --lib --locked` passed in 12.41
  seconds after the final constructor/configuration correction.
- The focused `font_language_override` suite passed 4/4 tests.
- The complete `font_` regression group passed 79/79 tests in 31.95 seconds;
  an earlier combined run exposed one transient existing FontFace fallback
  failure, and its isolated rerun plus this clean group rerun passed.
- The full locked `glass-browser` library gate passed 1,214 tests with 1
  ignored in 67.51 seconds (67.86 seconds wall including Cargo overhead).
- Formatting and diff checks passed. Release-documentation truth passed for
  1,166 Markdown documents with zero current-claim failures; documentation
  depth passed with 93 routed current guides and 19 substantive contracts;
  the TUI inventory passed with 15 implementation keys and 63 documentation
  markers; and documentation coverage passed with 346 full-product MCP tools
  (101 browser-only), 17 examples, and 22 public modules.
- The checkout remains local-only: no remote CI, push, release, tag, registry
  publication, or native/CDP parity certification is claimed.

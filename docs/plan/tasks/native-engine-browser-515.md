# Native engine browser slice 515: expand the font-variant shorthand

Status: complete locally on the current source line.

## Objective

Implement the bounded inherited CSS `font-variant` shorthand so it expands
atomically into the native longhands already supported by the text engine.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-514.md`

## Contract

- Accept the existing bounded `font-variant-ligatures`, `font-variant-caps`,
  `font-variant-position`, `font-variant-alternates`,
  `font-variant-east-asian`, and `font-variant-numeric` keyword groups in one
  shorthand declaration.
- Expand `normal` and `none` to their defined initial longhand values, and
  apply `inherit`, `initial`, `unset`, `revert`, and `revert-layer` to every
  longhand in the shorthand.
- Reject duplicate group values, mixed CSS-wide/ordinary values, unknown
  tokens, and malformed declarations without partially changing style state.
- Preserve normal declaration order and importance so a later longhand wins and
  an earlier shorthand is still overridden by a later declaration.
- Expose the computed shorthand through the stable CSSOM property inventory,
  while retaining each existing longhand projection and shaping mapping.
- Preserve font selection, fallback, measurement, rasterization,
  content-process compatibility, and the two-installable-crate boundary.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`

## Tradeoffs and explicit limits

The shorthand composes only the bounded longhand values already owned by the
native engine; it does not broaden any longhand's accepted grammar. It emits a
compact canonical serialization that omits default longhand groups. Full CSS
Fonts parsing, parameterized alternate feature values, language-specific and
vertical shaping, variable/color tables, WOFF2, and complete CSS Fonts/Web IDL
parity remain separate issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused `font_variant_shorthand` parser/cascade/CSSOM witnesses
- focused font regression group
- `RUST_MIN_STACK=8388608 cargo test --quiet -p glass-browser --lib --locked -- --test-threads=1`
- repository documentation validators and `git diff --check`

The slice is complete only when implementation, focused behavior, affected
package regression, documentation, and local resource accounting agree. No
remote CI, push, release, tag, registry publication, or native/CDP parity
certification is implied by this task.

## Results

- `cargo fmt --all -- --check` passed.
- `cargo check --quiet -p glass-browser --lib --locked` passed in 11.6 seconds
  after the final parser correction.
- The focused `font_variant_shorthand` suite passed 3/3 tests.
- The affected font regression group passed 75/75 tests.
- The full `glass-browser` library gate passed 1,210 tests, with 1 ignored, in
  62.56 seconds wall time.
- Release documentation, documentation depth, TUI shortcut, and documentation
  coverage validators all passed: 1,165 Markdown files, 93 routed/audited
  current guides, 15 implementation help keys, and 346 full-product MCP tools.
- `git diff --check` passed; no remote CI, push, release, tag, registry
  publication, or native/CDP parity certification was performed.

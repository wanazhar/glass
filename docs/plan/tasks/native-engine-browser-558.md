# Native-engine browser slice 558: fractional pixel font sizes

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Close the bounded CSS `font-size` fractional-pixel gap in the native inherited
style path while preserving the integer-pixel computed-style contract used by
layout, shaping, rasterization, and page-realm snapshots.

## Scope

- Accept positive CSS `px` values with up to three fractional decimal places
  for inherited element `font-size` declarations.
- Convert source pixels to the native integer-pixel value with checked
  thousandths and half-up rounding.
- Preserve the existing 1 through 256 px result bound and cascade fallback for
  values that round below 1 px or above 256 px.
- Keep viewport units, unit algebra, extra-precision CSS number grammar, and
  complete CSS `font-size` parity outside this slice.

## Contract

- `16.5px` resolves to 17 px and `16.499px` resolves to 16 px.
- `0.5px` resolves to the minimum native 1 px value; values that round below
  1 px are rejected.
- `256.499px` resolves to 256 px; `256.5px` is rejected so an inherited or
  lower-priority candidate wins.
- Four or more fractional digits remain fail-closed under the bounded native
  decimal grammar.

## Implementation

- `parse_font_size` routes the `px` branch through the existing bounded
  thousandths parser, applies checked half-up rounding, and retains the native
  1..=256 result filter.
- Existing `NativeFontSizeDeclarationValue::Pixels` and cascade resolution are
  unchanged; only the accepted absolute pixel syntax is widened.
- Parser coverage exercises half-up/down boundaries, minimum and maximum
  bounds, and excess precision. Document coverage exercises rounded values and
  out-of-range cascade fallback.

## Verification

- Focused parser coverage passed:
  `RUST_MIN_STACK=8388608 cargo test -p glass-browser --lib --locked
  font_size_parser_accepts_bounded_absolute_units -- --test-threads=1`
  reported 1 passed; the focused document cascade test reported 1 passed.
- The full locked native library suite passed with
  `RUST_MIN_STACK=8388608`: 1285 passed, 1 ignored, and 1284 filtered across
  the two emitted test suites.
- Package gates passed: `scripts/check-rust-workspace.sh fast-check`,
  `cargo build -p glass-browser --bin glass-browser --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts. Coverage passed for 1208 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules. Release truth passed for the same 1208 Markdown documents with
  current=83, previous-version hits=63, semantic hits=1367, and zero
  current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed after final edits.
- No CDP fallback, remote issue mutation, push, or release action was used.

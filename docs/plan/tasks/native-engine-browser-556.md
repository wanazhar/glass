# Native-engine browser slice 556: relative font-size units

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Extend the bounded inherited CSS `font-size` path from absolute lengths to the
common parent-relative `em` and percentage units without changing the native
integer-pixel computed-style or layout contract.

## Scope

- Accept positive bounded `em` and `%` element `font-size` declarations.
- Resolve each relative value against the already-computed parent font size,
  round the result half-up to integer pixels, and retain the native 1 through
  256 px bound.
- Preserve CSS-wide `inherit`, `initial`, `unset`, `revert`, and `revert-layer`
  behavior and fail closed when a relative candidate converts outside the native
  bound.
- Keep `rem`, viewport units, unit algebra, zero/negative values, and complete
  CSS font-size grammar outside this slice.

## Contract

- `1em` and `100%` represent a 1,000 milli-scale factor; decimal factors use
  the existing bounded thousandths grammar. Percentages normalize their
  percentage number into the same milli-scale representation.
- Relative values are multiplied by the inherited computed font size with
  checked integer arithmetic and half-up rounding. A result outside 1..=256 px
  blocks that candidate and lets the existing cascade continue to a lower
  candidate or inherited value.
- Relative scales are bounded to 1..=256x before entering cascade storage.
  `rem`, viewport-relative units, malformed precision, and unsupported values
  remain rejected without changing the existing absolute-unit behavior.

## Implementation

- `css.rs` stores font-size declarations as a private value enum containing
  absolute pixels or a bounded milli-scale relative factor.
- The computed-style resolver applies relative factors to the inherited parent
  size while preserving the existing CSS-wide declaration resolver and native
  integer `font_size` field consumed by layout, shaping, and rasterization.
- Parser coverage exercises `em`, percentage, zero, over-budget, and `rem`
  boundaries; document coverage verifies 1.5em and 125% under a 24 px parent and
  confirms an out-of-range relative result preserves the inherited size.

## Verification

- Focused relative parser coverage passed:
  `RUST_MIN_STACK=8388608 cargo test -p glass-browser --lib --locked
  font_size_parser_accepts_bounded_relative_units -- --test-threads=1`
  reported 1 passed; the focused computed-style cascade test reported 1
  passed.
- The full locked native library suite passed with
  `RUST_MIN_STACK=8388608`: 1284 passed, 1 ignored, and 1283 filtered across
  the two emitted test suites.
- Package gates passed: `scripts/check-rust-workspace.sh fast-check`,
  `cargo build -p glass-browser --bin glass-browser --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts. Coverage passed for 1206 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules. Release truth passed for the same 1206 Markdown documents with
  current=83, previous-version hits=63, semantic hits=1367, and zero
  current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed after final edits.
- No CDP fallback, remote issue mutation, push, or release action was used.

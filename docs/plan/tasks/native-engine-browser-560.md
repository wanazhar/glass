# Native-engine browser slice 560: additive font-size calc values

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Close the first native CSS unit-algebra gap for inherited `font-size` by
supporting bounded additive `calc()` expressions while preserving deferred
resolution against each element's parent, root, and viewport style context.

## Scope

- Accept bounded `calc()` expressions containing additive and subtractive
  `px`, CSS absolute units, `em`, percentages, `rem`, and viewport units.
- Preserve each term's unit context until computed-style resolution, then apply
  checked arithmetic and half-up integer-pixel rounding.
- Keep the existing 1 through 256 px bound and cascade fallback for non-positive
  or out-of-range final results.
- Keep multiplication/division, nested math functions, CSS variables, and
  complete CSS math grammar outside this slice.

## Contract

- Additive expressions accept at most 16 bounded terms and require each term to
  carry a supported length or font-relative unit; unitless terms and malformed
  operator sequences fail closed.
- `calc(1em + 2px)` combines the computed parent size with 2 px; percentage
  terms use the same parent-relative scale as standalone declarations, and
  `rem`/viewport terms retain their root/configured-viewport bases.
- The final expression is evaluated in checked fixed-point arithmetic, rounded
  half-up to the native integer-pixel model, and rejected when it is non-positive
  or outside 1..=256 px.

## Implementation

- `css.rs` adds `NativeFontSizeCalculation` coefficient storage, bounded
  additive expression parsing, and deferred resolution against inherited,
  root, and viewport context.
- Existing absolute/relative/viewport declaration paths remain unchanged; the
  new calculation variant participates in the same cascade candidate fallback.
- Parser coverage checks mixed-unit addition, subtraction, malformed and
  unsupported operators. Document coverage checks parent-relative, percentage,
  viewport, negative, and over-range results.

## Verification

- Focused parser coverage passed:
  `RUST_MIN_STACK=8388608 cargo test -p glass-browser --lib --locked
  font_size_parser_accepts_bounded_relative_units -- --test-threads=1`
  reported 1 passed; the focused additive calc cascade test reported 1 passed.
- The full locked native library suite passed with
  `RUST_MIN_STACK=8388608`: 1287 passed, 1 ignored, and 1286 filtered across
  the two emitted test suites.
- Package gates passed: `scripts/check-rust-workspace.sh fast-check`,
  `cargo build -p glass-browser --bin glass-browser --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts. Coverage passed for 1210 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules. Release truth passed for the same 1210 Markdown documents with
  current=83, previous-version hits=63, semantic hits=1367, and zero
  current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed after final edits.
- No CDP fallback, remote issue mutation, push, or release action was used.

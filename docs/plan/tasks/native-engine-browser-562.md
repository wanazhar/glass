# Native-engine browser slice 562: extra-precision font-size numbers

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Accept bounded CSS `font-size` and `calc()` numeric inputs with more than the
native three fractional decimal digits while retaining the existing checked
thousandth and integer-pixel result model.

## Scope

- Parse up to six fractional decimal digits for direct absolute/relative,
  root-relative, viewport-relative, and `calc()` font-size terms.
- Round the parsed value half-up to the existing thousandth fixed-point
  coefficients before unit conversion, product scaling, and cascade
  resolution.
- Reject malformed values and precision beyond six fractional digits; keep the
  native 1 through 256 px result bound and existing cascade fallback.
- Keep unrelated CSS numeric properties on their existing parser contract.

## Contract

- `1.000499` maps to `1_000` thousandths and `1.0005` maps to `1_001`; six
  fractional digits are accepted and a seventh fractional digit fails closed.
- Existing unit conversion, additive/product fixed-point arithmetic, half-up
  pixel rounding, and out-of-range fallback remain unchanged after the input
  precision conversion.
- Extra precision is retained through relative and `calc()` factors, while the
  final computed style remains an integer pixel value.

## Implementation

- `css.rs` adds a font-size-local decimal parser that rounds six-digit input to
  thousandths and routes direct units, relative units, calculation terms, and
  unitless product factors through it.
- Existing global `parse_decimal_milli` behavior is unchanged for unrelated
  CSS declarations.
- Parser coverage checks half-up precision and seventh-digit rejection;
  document coverage checks direct, relative, tiny, and over-precise cascade
  behavior.

## Verification

- Focused parser and document-cascade checks passed:
  `font_size_parser_accepts_extra_decimal_precision` and
  `extra_precision_font_sizes_round_into_native_model`.
- `RUST_MIN_STACK=8388608 cargo test -p glass-browser --lib --locked` passed:
  1290 tests passed across two suites, one ignored, and 1289 filtered.
- `scripts/check-rust-workspace.sh fast-check`,
  `cargo build -p glass-browser --bin glass-browser --locked`,
  `cargo build -p glass-dev --bin glass --locked`, and locked metadata
  generation passed.
- Documentation depth passed with 93 current guides and 19 substantive
  contracts; coverage passed with 1212 Markdown files, 346 full-product MCP
  tools, 101 browser-only tools, 17 examples, and 22 public modules; release
  truth passed with 83 current documents, 63 previous-version hits, 1367
  semantic audit hits, and zero current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- No CDP fallback, remote issue mutation, push, or release action was used.

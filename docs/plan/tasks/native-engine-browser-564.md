# Native-engine browser slice 564: chained font-size products

Status: implementation checkpoint; issue #40 remains open.

## Objective

Extend the bounded native CSS `font-size` `calc()` product parser from one
unitless multiplication or division factor to a checked chain of factors while
preserving nested calculation groups and fail-closed dimensional rules.

## Scope

- Evaluate up to eight product operands left to right.
- Accept one deferred font-size dimension expression with unitless factors on
  either side for multiplication.
- Accept division only by unitless factors, including chained division.
- Preserve nested `calc()` groups as single product operands.
- Reject dimension/dimension products, division by zero, unitless-only results,
  malformed operators, and product chains beyond the bounded operand count.

The fixed-point thousandth representation and native 1 through 256 pixel
computed-style bound remain unchanged. Full CSS math grammar, arbitrary
precision, dynamic viewport recomputation, and complete CSS font-size parity
remain issue #40 gates.

## Verification

Focused native checks passed:

- `font_size_parser_accepts_bounded_relative_units`
- `product_and_nested_calc_font_sizes_resolve`

The full native library suite passed with
`RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
1292 tests passed across two suites, one ignored, and 1291 filtered.

The locked workspace fast check, `glass-browser` binary build, `glass-dev`
binary build, and locked metadata generation passed.

Documentation depth passed with 93 current guides and 19 substantive
contracts; coverage passed with 1214 Markdown files, 346 full-product MCP
tools, 101 browser-only tools, 17 examples, and 22 public modules; release
truth passed with 83 current documents, 63 previous-version hits, 1367
semantic audit hits, and zero current-claim failures.

`cargo fmt --all -- --check` and `git diff --check` passed.

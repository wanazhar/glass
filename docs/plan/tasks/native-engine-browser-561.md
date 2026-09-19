# Native-engine browser slice 561: multiplicative font-size calc values

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Extend the bounded native `font-size` `calc()` algebra from additive terms to
unitless multiplication, division, and nested `calc()` grouping.

## Scope

- Accept one supported dimension expression multiplied by a bounded unitless
  factor in either operand order.
- Accept supported dimension expressions divided by a non-zero bounded unitless
  factor.
- Preserve nested `calc()` parentheses while finding outer additive operators.
- Keep chained products, dimension-by-dimension multiplication, negative
  factors, CSS variables, and complete CSS math grammar outside this slice.

## Contract

- `calc(1em * 2)`, `calc(2 * 1em)`, and `calc(24px / 2)` resolve with checked
  fixed-point coefficient scaling; division by zero and dimension/dimension
  products fail closed.
- Nested additive expressions can be grouped and then scaled, for example
  `calc(calc(1em + 2px) * 2)`.
- Product factors use the existing bounded decimal grammar and final values
  still round half-up into the native 1 through 256 px result range; invalid or
  out-of-range results fall through to the existing cascade.

## Implementation

- `css.rs` scales all fixed-point calculation coefficients for product/division
  operations, recognizes the valid dimension/unitless operand arrangements, and
  tracks parentheses depth while parsing additive operators.
- Nested `calc()` terms recurse through the existing calculation parser, while
  multiple product operators or unsupported operand combinations remain
  rejected.
- Parser coverage checks multiplication order, division, nesting, zero and
  chained-product rejection. Document coverage checks multiplication, reverse
  multiplication, division, nested grouping, and fallback.

## Verification

- Focused parser and document-cascade checks passed:
  `font_size_parser_accepts_bounded_relative_units` and
  `product_and_nested_calc_font_sizes_resolve`.
- `RUST_MIN_STACK=8388608 cargo test -p glass-browser --lib --locked` passed:
  1288 tests passed across two suites, one ignored, and 1287 filtered.
- `scripts/check-rust-workspace.sh fast-check`,
  `cargo build -p glass-browser --bin glass-browser --locked`,
  `cargo build -p glass-dev --bin glass --locked`, and locked metadata
  generation passed.
- Documentation depth passed with 93 current guides and 19 substantive
  contracts; coverage passed with 1211 Markdown files, 346 full-product MCP
  tools, 101 browser-only tools, 17 examples, and 22 public modules; release
  truth passed with 83 current documents, 63 previous-version hits, 1367
  semantic audit hits, and zero current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- No CDP fallback, remote issue mutation, push, or release action was used.

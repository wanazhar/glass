# Native-engine browser slice 555: absolute font-size units

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Close the bounded CSS `font-size` absolute-unit gap called out by issue #40 while
preserving the native engine's integer-pixel computed-style contract.

## Scope

- Accept CSS `pt`, `pc`, `in`, `cm`, and `mm` values for inherited element
  `font-size` declarations alongside the existing bounded integer `px` path.
- Resolve absolute units through the CSS 96 dpi reference pixel, round the
  resulting pixel value half-up, and retain the native 1 through 256 px bound.
- Keep malformed, zero, over-budget, fractional-`px`, relative, percentage,
  viewport, and other unsupported values fail-closed.
- Cover parser conversions and computed-style cascade behavior without claiming
  relative-unit resolution or complete CSS font-size parity.

## Contract

- `pt` maps to 96/72 px, `pc` to 16 px, `in` to 96 px, `cm` to 96/2.54 px,
  and `mm` to 96/25.4 px.
- Decimal absolute values use the existing bounded thousandths parser; the
  converted pixel result rounds half-up before the 1..=256 bound is applied.
- Existing integer `px` behavior remains unchanged. `em`, `rem`, `%`, viewport
  units, malformed numeric precision, and values that convert outside the bound
  return no declaration and preserve the inherited style.

## Implementation

- `css.rs::parse_font_size` now recognizes the five absolute units, performs
  checked conversion using integer thousandths, and filters the rounded result
  through the existing native pixel bounds.
- CSS parser coverage exercises every supported absolute unit and rejection
  bounds; computed-style coverage verifies a `9pt` declaration resolves to 12
  native pixels while the existing inheritance and CSS-wide reset behavior stays
  intact.

## Verification

- Focused parser coverage passed:
  `RUST_MIN_STACK=8388608 cargo test -p glass-browser --lib --locked
  font_size -- --test-threads=1` reported 2 passed; the focused computed-style
  cascade test reported 1 passed.
- The full locked native library suite passed with
  `RUST_MIN_STACK=8388608`: 1283 passed, 1 ignored, and 1282 filtered across
  the two emitted test suites.
- Package gates passed: `scripts/check-rust-workspace.sh fast-check`,
  `cargo build -p glass-browser --bin glass-browser --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts. Coverage passed for 1205 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules. Release truth passed for the same 1205 Markdown documents with
  current=83, previous-version hits=63, semantic hits=1367, and zero
  current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed after final edits.
- No CDP fallback, remote issue mutation, push, or release action was used.

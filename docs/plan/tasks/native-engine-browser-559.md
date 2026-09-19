# Native-engine browser slice 559: viewport-relative font sizes

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Close the bounded CSS viewport-unit gap for inherited native `font-size` while
keeping page styles, content-process snapshots, layout, and rasterization on
one validated viewport contract.

## Scope

- Accept bounded `vw`, `vh`, `vmin`, and `vmax` `font-size` declarations.
- Resolve each unit against the configured document viewport, using checked
  thousandths and half-up integer-pixel conversion.
- Carry the validated viewport through native document style computation and the
  content-process wire, preserving the existing 1 through 256 px result bound
  and cascade fallback.
- Keep `calc()`/unit algebra, dynamic viewport recomputation, and complete CSS
  `font-size` parity outside this slice.

## Contract

- `1vw` is one hundredth of viewport width, `1vh` one hundredth of height,
  `1vmin` one hundredth of the smaller dimension, and `1vmax` one hundredth of
  the larger dimension.
- The bounded decimal factor uses the existing thousandths grammar. Viewport
  pixel conversion rounds half-up and rejects results below 1 px or above
  256 px so an inherited or lower-priority candidate wins.
- A parsed document defaults to the validated default viewport; the content
  loader installs the configured viewport before computing the document wire.
  Wire snapshots carry the viewport with serde-default compatibility for older
  snapshots, and changing a document viewport invalidates computed styles.

## Implementation

- `css.rs` adds viewport font-size units, passes `Viewport` through
  `NativeInheritedStyle`, and resolves viewport candidates with checked
  dimension-specific scaling.
- `config.rs` makes the bounded `Viewport` wire-serializable. `dom.rs` stores,
  validates, serializes, restores, and invalidates on viewport changes.
- `content_process.rs` installs the loaded viewport before style snapshots are
  emitted. Parser coverage validates all four units and bounds; document
  coverage checks width, height, min/max, and out-of-range fallback.

## Verification

- Focused parser coverage passed:
  `RUST_MIN_STACK=8388608 cargo test -p glass-browser --lib --locked
  font_size_parser_accepts_bounded_relative_units -- --test-threads=1`
  reported 1 passed; the focused viewport cascade test reported 1 passed.
- The full locked native library suite passed with
  `RUST_MIN_STACK=8388608`: 1286 passed, 1 ignored, and 1285 filtered across
  the two emitted test suites.
- Package gates passed: `scripts/check-rust-workspace.sh fast-check`,
  `cargo build -p glass-browser --bin glass-browser --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts. Coverage passed for 1209 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules. Release truth passed for the same 1209 Markdown documents with
  current=83, previous-version hits=63, semantic hits=1367, and zero
  current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed after final edits.
- No CDP fallback, remote issue mutation, push, or release action was used.

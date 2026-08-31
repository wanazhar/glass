---
id: native-engine-054
scope: glass-browser/native-engine/fixed-cell-text-decoration
status: done
depends-on: [native-engine-053]
---

# Native bounded fixed-cell text decoration

## Objective

Add one useful text-paint decoration to the existing fixed-cell native
renderer. Supported text should be able to opt into an inherited one-pixel
underline while preserving the current layout, display-list ownership,
software replay, hit testing, and scrolling contracts.

## Contract

The native CSS grammar accepts `text-decoration: none` and
`text-decoration: underline`. The bounded value is inherited through the
existing DOM style walk; the initial value is `none`, and an explicit child
`none` clears the bounded inherited decoration. Values containing multiple
lines or unsupported lines such as `overline`, `line-through`, or `blink`, as
well as decoration colors, styles, thicknesses, offsets, and CSS-wide values,
are diagnosed as unsupported and do not change the cascade.

Each fixed-cell text display command carries the computed underline bit. The
software rasterizer draws one logical pixel at the fixed glyph baseline
offset, spanning the bounded fixed-cell run width, using the run's existing
RGBA color and clip. Line fragments underline independently; empty runs do
not emit paint. The decoration is available for direct text and descendant
text nodes, including content flowing through `display:contents`.

Text decoration does not change text width, line breaking, line height, box
dimensions, root overflow, point hit testing, actionability, scrolling,
opacity-group boundaries, capture, revisions, or navigation. It does not
implement font metrics, descender-aware positioning, shaping, bidi, text
decoration propagation parity, decoration skip behavior, anti-aliasing,
physical pixels, or arbitrary font/text decoration support. No dependency or
second text/layout owner is introduced.

## Tradeoffs

- A fixed baseline offset keeps the result deterministic and dependency-free,
  but it is not a font-aware underline position and may not match browser
  descender or font metrics.
- Carrying the bit in the immutable text command keeps raster replay
  revision-stable and avoids re-reading mutable style during painting, while
  intentionally limiting the command to one decoration variant.
- Treating the bounded value as inherited makes nested fixture behavior easy
  to inspect, but differs from the full CSS text-decoration propagation
  model; that limitation remains visible in diagnostics and documentation.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parsing, invalid-value diagnostics, selector/inline cascade, and inherited
  `none`/`underline` values are covered by unit tests;
- direct text, nested text, `display:contents`, explicit clearing, and
  multiline fixed-cell fragments are covered by integration tests;
- the display-list command carries the expected decoration and the surface
  contains a clipped, alpha-aware underline at the deterministic baseline;
- existing color, opacity, clipping, scrolling, capture, layout, hit-test,
  navigation, and revision behavior remains green;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

Implemented locally in `33a773d` (`feat(native-engine): add fixed-cell text
decoration`). The bounded parser/cascade, DOM inheritance, immutable display
command bit, clipped alpha-aware raster underline, and direct/nested/
`display:contents` coverage are all present without a new dependency.

Post-implementation local evidence:

- `cargo test -p glass-browser --features native-engine --test native_engine
  -- --nocapture`: 68 passed, 0 failed;
- `cargo test -p glass-browser --features native-engine --lib
  surface_draws_alpha_underlines_across_fixed_cells_and_clips_them --
  --nocapture`: 1 passed, 836 filtered out;
- the native-engine library inventory contains 55 unit tests, including the
  parser, inheritance, and raster underline tests;
- `RUST_MIN_STACK=4194304 scripts/check-rust-workspace.sh test`: passed after
  a warm rerun; the first clean run exposed the existing cold-start
  rust-analyzer diagnostic timing race, then the exact test and complete
  `glass-dev` library suite passed 365/365;
- strict all-target/all-feature Clippy passes for both workspace packages,
  plus the browser no-default-feature pass;
- formatting, diff, and documentation/release validators pass after the
  synchronized docs checkpoint;
- issue #40 is updated under the authenticated `wanazhar` account with this
  implementation, validation, and cleanup evidence; remote CI remains pending
  because this local branch has not been pushed;
- exact regenerable Cargo output is reclaimed after the final Cargo command;
  no long-lived Glass process is terminated.

No later native-engine slice is active in this checkpoint.

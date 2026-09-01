---
id: native-engine-060
scope: glass-browser/native-engine/inherited-font-style
status: complete
depends-on: [native-engine-059]
---

# Native bounded inherited font style

## Objective

Add a bounded inherited `font-style` presentation slice to the existing
fixed-cell text-flow and software-raster owners. Supported local fixtures
should be able to request deterministic normal or italic text treatment while
keeping layout, semantics, hit testing, display-list projection, and capture
on the existing coordinate path.

## Contract

The native CSS grammar accepts only `normal` and `italic` for `font-style`.
Both values are inherited through the existing DOM style walk; the initial
value is `Normal`. `oblique`, angle-bearing values, CSS-wide keywords, and
malformed values are diagnosed as unsupported and do not change the computed
value.

`Normal` retains the existing fixed 5x7 ASCII glyph replay. `Italic` applies a
deterministic row-dependent horizontal shear to each set glyph pixel: rows
near the top of the seven-row glyph move right by a bounded amount and rows
near the baseline retain their original column. The shear is a presentation
operation only; the fixed six-cell glyph advance, letter spacing, word spacing,
line height, wrapping, glyph origin, and all document geometry remain
unchanged. Bold dilation composes with the same italic row shift, with each
pixel painted at most once per row.

Italic pixels are clipped by the existing viewport and ancestor rectangles.
They never paint outside the same logical raster surface or create a second
text/layout owner. Underline geometry continues to use the measured fixed-cell
run width rather than the sheared glyph pixels.

The resolved style is carried on immutable text display commands. Raster replay
consumes that command value rather than re-reading mutable document style, so
direct display-list inspection, software painting, opacity groups, scroll
projection, and PNG capture observe the same presentation choice.

Semantic source text, accessible names, text-fragment matching, revisions,
locators, controls, text origins, measured overflow, and point hit testing
remain unchanged because the bounded style does not alter advance metrics or
layout coordinates.

This is a deterministic native fixture rule, not CSS `font-style` or browser
text-rendering parity. It does not implement oblique angles, font selection,
font loading, font metrics, real font faces, variable fonts, Unicode shaping,
grapheme clusters, bidi, anti-aliasing, synthetic-style policy, or
platform-specific glyph behavior. Unsupported non-ASCII glyph fallback
remains unchanged.

## Tradeoffs

- A row-dependent shear makes italic text observable without importing a font
  dependency or changing the established integer-cell geometry, but it is a
  raster approximation and cannot reproduce real italic faces or angles.
- Keeping advances and all layout coordinates unchanged preserves the shared
  hit-test/overflow contract and keeps the feature cheap to reason about, but
  the painted top rows may approach the next fixed cell without reserving
  additional layout width.
- Carrying the resolved value through immutable commands keeps replay stable
  across scroll, opacity, and capture, but adds another presentation field to
  the experimental display/raster contract.
- Accepting only `normal` and `italic` keeps diagnostics and build scope
  bounded, but `oblique`, angle-bearing styles, font-family selection, and
  browser text behavior remain unsupported and visible.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parser, initial value, stylesheet/inline cascade, inheritance, child
  override, and invalid-value diagnostics are covered by unit tests;
- normal and italic text retain identical layout rectangles, origins, advance,
  wrapping, text fragments, root overflow, and hit-test results;
- display-list commands carry the resolved normal/italic value and preserve it
  through opacity groups, scrolling, and capture;
- italic replay applies the deterministic row shear, bold+italic composes
  without duplicate alpha blending, and normal replay remains byte-compatible
  with the pre-slice glyph path;
- clipping, alpha compositing, underline, letter spacing, and word spacing
  compose through the existing raster owners;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

Implementation is committed locally as `2992eb8` (`feat(native-engine): add
bounded font style`). The native CSS parser accepts and inherits `normal` and
`italic`, immutable text commands carry the resolved italic flag, and the
software rasterizer applies the bounded row-dependent shear without changing
fixed-cell advances or geometry.

- `cargo test -p glass-browser --features native-engine --test native_engine
  --locked -- --nocapture`: 75 passed;
- `RUST_MIN_STACK=4194304 cargo test -p glass-browser --features native-engine
  --lib --locked -- --nocapture`: 849 passed, 1 ignored;
- focused parser, raster, diagnostic, and integration tests passed, including
  invalid-value diagnostics, inheritance/override, command metadata, normal
  versus italic geometry, clipping, bold+italic composition, and alpha
  single-paint coverage;
- `cargo clippy -p glass-browser --all-targets --all-features --locked --
  -D warnings` and the corresponding `--no-default-features` gate passed;
- `cargo fmt --all -- --check` and `git diff --check` passed;
- the synchronized architecture, analysis, plan, and public capability docs
  record the same bounded rule and its exclusions;
- documentation gates passed: version sync, feature parity, release
  documentation/current claims (474 Markdown documents, 0 current-claim
  failures), documentation depth, TUI shortcuts, reliability matrix,
  public read-only adapters, Web IR corpus, and documentation coverage (474
  Markdown documents, 345 full-product MCP tools, 100 browser-only tools, 17
  examples, 22 public modules);
- `RUSTDOCFLAGS='-D warnings' cargo doc --all-features --locked --no-deps`
  passed, and the debug `glass-dev` binary required by coverage rebuilt
  successfully;
- no dependency or third crate was added; remote CI remains pending until this
  local branch is pushed separately.

Exact regenerable Cargo outputs are reclaimed after the documentation and
Rustdoc gates.

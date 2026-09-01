---
id: native-engine-058
scope: glass-browser/native-engine/inherited-letter-spacing
status: complete
depends-on: [native-engine-057]
---

# Native bounded inherited letter spacing

## Objective

Add a bounded inherited `letter-spacing` presentation slice to the existing
fixed-cell text-flow owner. Supported local fixtures should be able to widen
the advance of rendered characters while keeping wrapping, alignment, text
fragments, display-list output, raster replay, hit testing, and root overflow
on one shared coordinate path.

## Contract

The native CSS grammar accepts only non-negative fixed-pixel values for
`letter-spacing`, including `0px`, bounded by the existing native dimension
limit. The property is inherited through the existing DOM style walk; its
initial value is `0px`. Negative lengths, percentages, unitless values,
font-relative units, `normal`, CSS-wide keywords, and other syntax are
diagnosed as unsupported and do not change the computed value.

The bounded native model adds the letter-spacing advance after every rendered
fixed-cell character in each emitted text fragment. This includes ASCII
spaces and the final character of a fragment. Fragment boundaries, hard line
breaks, separate direct-text nodes, separate inline items, and source
whitespace ownership remain hard boundaries; spacing never creates text or
joins independent flow owners. `word-spacing` remains an additional advance
after rendered ASCII spaces, so a space receives both values when both are
non-zero.

The measured advance is applied before soft wrapping, preformatted chunking,
text-fragment coordinates, text alignment, display-list projection,
root-overflow measurement, and raster replay. Immutable text display commands
carry the computed letter and word spacing so the fixed glyph rasterizer uses
the same bounded character arithmetic; underline spans include that measured
run width. Semantic DOM text, accessible names, compact evidence, locators,
revisions, navigation, action semantics, block geometry, line height,
opacity, color, decoration, indentation, and word-spacing ownership remain
unchanged except for shared text coordinates and any resulting root
horizontal extent.

Spacing uses saturating bounded arithmetic. The existing fixed-cell glyph
advance remains the base for every character, and letter-spacing is an
additional presentation advance rather than a replacement width. No second
text/layout owner or dependency is introduced.

This is a deterministic native fixture rule, not CSS typographic parity. It
does not implement pair-boundary spacing, cross-fragment/cross-node joining,
negative spacing, `word-spacing` keywords or negative values, Unicode
shaping/metrics, grapheme clusters, bidi/logical writing modes, justification,
or browser CSS conformance.

## Tradeoffs

- Attaching the value to every emitted fixed-cell character keeps flow,
  fragments, alignment, overflow, paint, hit testing, and raster arithmetic
  identical, but intentionally exposes a bounded native rule rather than
  browser pair-boundary semantics.
- Carrying both spacing values on immutable text commands keeps replay
  deterministic and independent of mutable style, but expands the command and
  raster paint shape for another experimental presentation value.
- Rejecting negative, relative, percentage, `normal`, and Unicode/font-aware
  forms preserves the existing integer budget and build footprint, but leaves
  many browser-authored typography cases unsupported and diagnostically
  visible.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parser, invalid-value diagnostics, stylesheet/inline cascade, initial value,
  inheritance, and child override behavior are covered by unit tests;
- normal, `nowrap`, `pre-line`, `pre`, and `pre-wrap` rendered characters use
  the same bounded letter-spacing arithmetic;
- letter and word spacing compose on ASCII spaces without mutating semantic
  source text or changing whitespace ownership;
- wrapping, preformatted chunking, hard-break reset, `text-indent`, and
  `text-align` preserve the documented ownership boundaries;
- text fragments, display-list spacing metadata, underline extents, raster
  glyph positions, hit testing, and root overflow share the measured result;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

- Implementation is committed locally as `cb191a3` (`feat(native-engine):
  add bounded letter spacing`); the change stays inside `glass-browser` and
  adds no dependency or third crate.
- `cargo test -p glass-browser --features native-engine --test native_engine
  --locked -- --nocapture`: 73 passed.
- `RUST_MIN_STACK=4194304 cargo test -p glass-browser
  --features native-engine --lib --locked -- --nocapture`: 843 passed, 1
  ignored.
- `cargo clippy -p glass-browser --all-targets --all-features --locked --
  -D warnings` and the corresponding `--no-default-features` gate passed.
- `cargo fmt --all -- --check` and `git diff --check` passed. Public
  capability docs and the architecture/analysis/plan records are synchronized
  with this completed boundary.
- Remote CI remains pending until this local branch is pushed; no push, tag,
  publication, or release is part of this epic checkpoint.

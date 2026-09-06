---
id: native-engine-140
scope: glass-browser/native-engine/cascade-layers-inherited-text-spacing-revert-layer
status: planned
depends-on: [native-engine-139]
---

# Native bounded inherited text-spacing `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-139 to
add standalone, case-insensitive `revert-layer` to the existing inherited
`word-spacing` and `letter-spacing` declarations. The two owners must resolve
independently while preserving their existing fixed-cell text-flow, wrapping,
alignment, display-list, raster, overflow, capture, hit-test, and
semantic/source-order contracts.

## Context

The native engine already accepts bounded non-negative integer-pixel values for
these inherited properties and carries them through the DOM style walk. Their
stylesheet and inline declarations currently keep only one winning concrete
candidate, so a higher-priority rollback declaration cannot expose a lower
candidate or the inherited parent value. This slice adds only private
declaration/candidate state and reuses the existing bounded first-appearance
15-layer registry and generic inherited-text resolver.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-3/#word-spacing-property>
- <https://www.w3.org/TR/css-text-3/#letter-spacing-property>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-139.md`

## Contract

### Declaration and cascade state

- `word-spacing` and `letter-spacing` accept one standalone,
  case-insensitive `revert-layer` token in addition to their existing bounded
  non-negative integer-pixel grammars. The rollback representation is private;
  the public computed values remain finite `u32` pixel values.
- Each property gets its own bounded candidate sequence. A rollback blocks
  only the current bounded layer for that property and resolves through its
  lower concrete candidate; if no concrete candidate remains, it resolves
  through the inherited parent value. The root fallback remains `0px` for
  both properties.
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer; unlayered and inline
  declarations remain above named layers. Repeated rollback, descendants,
  same-block valid declarations, and invalid-later-declaration preservation
  remain explicit behavior.
- `!important` is stripped by the existing bounded parser and is not promoted
  to a separate origin; no generic CSS-wide keyword engine is introduced.

### Existing owners preserved

- `word-spacing` continues to add its resolved finite advance only after
  rendered ASCII spaces in collapsed and supported preformatted flow.
- `letter-spacing` continues to add its resolved finite advance after every
  rendered fixed-cell character in each emitted fragment and composes with
  word spacing.
- The measured advance remains the shared input to wrapping, fragments,
  alignment/justification, display-list metadata, raster glyph positions,
  point hit testing, root overflow, capture, and inline subtree translation.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, fixed-cell, and bounded. It
does not add negative or relative spacing, `normal`, percentages, fractional
lengths, pair-boundary or cross-fragment semantics, Unicode shaping or
grapheme metrics, bidi, language-specific word boundaries, writing modes,
multiple origins, animation, script, or browser-wide CSS parity.

## Tradeoffs

- Reusing the generic private inherited-text wrapper and resolver avoids a
  second cascade implementation, at the cost of grouping two arithmetic
  properties under a less property-specific private type.
- Two fixed candidate arrays add a small bounded style-walk cost and stack
  state, but keep each property independent and avoid changing public computed
  style or artifact structures.
- Grouping the two text-flow arithmetic owners reduces checkpoint/build
  overhead; focused tests must still distinguish word-space-only,
  letter-space-only, and composed spacing behavior before the shared full
  native gate.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, negative, fractional, relative, and unsupported spacing
  forms for both properties;
- named-layer priority, repeated rollback, unlayered/inline precedence,
  inherited descendants, root fallback, same-block order, and invalid-later
  declarations for each owner;
- collapsed and supported preformatted spacing, wrapping, alignment/
  justification, display-list/raster output, overflow/capture, point hit
  testing, and unchanged semantic/source order;
- absence of false unsupported-value diagnostics and no public rollback
  keyword leakage; and
- focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.

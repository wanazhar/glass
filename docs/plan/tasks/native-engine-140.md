---
id: native-engine-140
scope: glass-browser/native-engine/cascade-layers-inherited-text-spacing-revert-layer
status: complete
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
these inherited properties and carries them through the DOM style walk. This
slice replaces their single-winner declaration state with private per-property
candidates so a higher-priority rollback declaration can expose a lower
candidate or the inherited parent value. It reuses the existing bounded
first-appearance 15-layer registry and generic inherited-text resolver.

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

## Implementation

Implemented in `7d40cf87598f05c61b02f27cf8713dd9ce5fadea` with the design
checkpoint `34f8ec1a0e7208f15194631d402c713583b5b967`. The implementation adds
private candidate arrays and declaration wrappers for both spacing properties,
keeps the public finite `u32` values unchanged, and hardens the shared parser
to preserve earlier valid inherited-text declarations when a later declaration
is invalid. Existing spacing, wrapping, alignment, display-list, raster,
overflow, capture, hit-test, and semantic/source-order owners remain the
consumers. No dependency, feature, public schema, or crate-boundary changes
were made.

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

The completed gate covered:

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
- locked focused `glass-browser` check passed;
- inherited-text parser/cascade tests passed 5/5, including invalid-later
  preservation, and the spacing consumer integration regression passed 1/1
  with 176 tests filtered;
- full native integration passed 177/177;
- the stack-adjusted full feature-enabled `glass-browser` library passed 945
  tests with 1 ignored and 0 failures;
- strict affected-package Clippy passed with `-D warnings`;
- formatting and final static documentation gates passed: 554 Markdown
  documents, 83 current documents, 57 previous-version hits, 649 semantic
  audit hits, and zero current-claim failures; coverage reported 345
  full-product MCP tools (100 browser-only), 17 examples, and 22 public
  modules; depth reported 93 current guides and 19 substantive contracts;
  parity reported 14 capabilities across 4 targets; TUI reported 15
  implementation keys and 63 documentation markers; adapters reported 5;
  reliability reported 6 scenarios across 4 targets; and Web IR reported
  8 fixtures, 8 scenarios, and 11 categories;
- the temporary `/tmp/glass-140-focused` target measured 7,084,596,446 logical
  bytes across 7,120 files and 925 directories, with no open handles or
  active Cargo/rustc/Clippy consumers; the exact target and 4,742-byte audit
  directory were removed with bounded `find -P ... -xdev -depth -delete`,
  reclaiming 7,105,167,360 bytes of measured `/tmp` free space; and
- remote CI remains unclaimed because the checkout is local-only and no push
  was authorized.

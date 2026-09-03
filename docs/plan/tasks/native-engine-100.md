---
id: native-engine-100
scope: glass-browser/native-engine/text-align-logical
status: ready
depends-on: [native-engine-099]
---

# Native bounded logical text alignment

## Objective

Add the bounded logical `text-align:start|end` values to the native engine's
existing fixed-cell inline-flow owner. Resolve those values against the
inherited `direction:ltr|rtl` state delivered by 099, while preserving the
physical behavior of `left|center|right`, source/semantic order, and the
existing no-bidi/shaping boundary.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-099.md`
- [CSS Text Module Level 3: text-align](https://www.w3.org/TR/css-text-3/#text-align-property)
- [CSS Writing Modes Level 4: direction](https://www.w3.org/TR/css-writing-modes-4/#propdef-direction)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts only the additional bounded logical values
`text-align:start` and `text-align:end`. The property remains inherited
through the existing DOM style walk. Its initial/fallback value remains the
current physical-left behavior for compatibility with the native engine's
existing fixed-cell model; this slice does not silently change omitted
`text-align` output.

For eligible horizontal-tb fixed-cell inline flow:

- `start` aligns the line's inline content to the physical left edge under
  `direction:ltr` and to the physical right edge under `direction:rtl`;
- `end` maps to the opposite physical edge;
- `left`, `right`, and `center` retain their existing physical semantics and
  are not reinterpreted through direction;
- wrapped lines resolve their logical alignment independently through the
  existing line flush owner, including direct text, supported inline boxes,
  hard breaks, whitespace modes, spacing, indent, overflow, and vertical
  alignment;
- every shifted inline item subtree and text run continues through the same
  layout, display-list, raster, viewport, hit-test, root-overflow, capture,
  scroll, and semantic consumers; source, semantic, and keyboard order remain
  unchanged;
- selector and inline cascade precedence, inherited direction overrides, and
  unsupported-value diagnostics remain typed and bounded.

The slice remains restricted to the current horizontal-tb, integer-pixel,
fixed-cell implementation. Unicode bidi resolution, glyph shaping, mixed bidi
runs, `unicode-bidi`, `text-align:justify|match-parent|justify-all`, logical
properties, vertical writing modes, grid, floats, and browser-wide text
conformance remain outside the contract and retain fail-closed fallback.

## Tradeoffs

- Reusing `FlowCursor` and its line-flush translation keeps logical alignment
  consistent across text, inline boxes, paint, hit testing, overflow, and
  capture without introducing a second geometry owner.
- Keeping physical `left|right` distinct avoids a breaking reinterpretation of
  existing native documents; callers that want direction-aware alignment must
  opt into `start|end`.
- The feature uses the inherited base direction as the sole logical mapping
  input. It does not inspect the first strong character or reorder text, so it
  provides useful layout alignment without claiming bidi paragraph behavior.
- Omitted `text-align` continues to use the established native default rather
  than adopting the broader CSS initial-value model in this isolated slice.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation gate will include:

- bounded parser, declaration, cascade, inheritance, inline precedence, and
  unsupported-value diagnostics for `text-align:start|end`;
- focused ltr/rtl start/end layout tests for direct text, wrapped lines, and
  supported inline boxes, with physical left/right/center regressions;
- display-list, software-raster, point-hit, overflow/viewport projection,
  capture, scroll, semantic/source-order, and nested-direction consumers;
- explicit tests proving direction-aware alignment does not reorder source text
  or add normal-flow Unicode bidi behavior;
- `cargo fmt --all -- --check`, focused and full native integration tests,
  feature-enabled library tests with the documented explicit stack, strict
  all-feature and no-default-feature Clippy, warning-denied rustdoc, locked
  binaries, package/dependency gates, fuzz checking, documentation/release
  validators, and exact isolated-target cleanup.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.

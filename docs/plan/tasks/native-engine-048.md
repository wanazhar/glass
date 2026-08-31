---
id: native-engine-048
scope: glass-browser/native-engine/clip-aware-root-overflow
status: done
depends-on: [native-engine-047]
---

# Native bounded clip-aware root overflow

## Objective

Correct the existing measured root horizontal overflow so text contributes
only through the same bounded `overflow:hidden`/`overflow:clip` intersection
already consumed by paint, viewport projection, and point hit testing.

## Contract

`NativeLayoutSnapshot::content_width` continues to derive from visible layout
boxes and text output, but a non-empty, non-truncated text run is measured
through its document-space text rectangle intersected with the existing
ancestor clip. A wide run fully clipped by a bounded overflow ancestor cannot
create a root horizontal scroll range; a partially clipped run contributes no
more than its visible right edge. Unclipped `nowrap`/`pre` output retains the
045 horizontal extent behavior.

The change reuses the existing layout-owned clip calculation and does not add
a second clipping or geometry owner. Root x/y scroll state, viewport
projection, point hit testing, display-list translation, raster replay,
actions, effects, and history retain their current contracts.

Hidden, non-layout, empty, and truncated output does not create a new extent
beyond the existing visible-layout rules. Nested scrolling, scrollbars,
axis-specific overflow, visible-overflow parity, and font metrics remain
outside the slice.

## Tradeoffs

- Measuring the clipped text rectangle keeps root overflow consistent with
  the existing paint/projection/hit-test owner, but does not implement a
  browser-style overflow propagation model.
- Excluding fully clipped runs avoids false scroll affordances, while the
  existing visible layout box still contributes its own bounded right edge.
- The fix remains dependency-free and keeps the fixed-cell native renderer
  limitation explicit.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and task docs

## Verification

- clipped wide text cannot increase root `content_width` or max x offset;
- partially clipped and unclipped wide text preserve the measured extent;
- existing clip-aware viewport, hit-test, display-list, raster, scroll, and
  history behavior remains green;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass.

## Completion evidence

Implementation and focused validation are complete locally. The code and
synchronized docs are committed as a focused Conventional Commit; the issue
#40 checkpoint follows before the next slice.

Validation evidence:

- focused clip-aware overflow integration test: 1 passed;
- full native integration suite: 61 passed;
- native-engine module unit suite: 43 passed;
- strict Clippy passed with all features and with no default features;
- `cargo fmt --all -- --check` and `git diff --check` passed;
- documentation depth, release-documentation, version-sync, and
  feature-parity validators passed: 93 current guides, 462 Markdown
  documents, 0 current-claim failures, synchronized 0.3.14 versions, and 14
  capabilities across 4 targets;
- no CLI/MCP inventory changed, so binary documentation coverage remains
  covered by the existing release gates.

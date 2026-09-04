---
id: native-engine-110
scope: glass-browser/native-engine/text-decoration-thickness
status: design-ready
depends-on: [native-engine-109]
---

# Native bounded text-decoration-thickness

## Objective

Expose a bounded inherited positive-pixel `text-decoration-thickness` value
for the existing fixed-cell decoration lines. The value must share the
current immutable text command, line origin, clipping, and software-raster
owner without introducing a second decoration geometry path.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-109.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `text-decoration-thickness` with one
case-insensitive positive fixed-pixel value from `1px` through `4px`. The
computed value is bounded-inherited through the existing fixed-cell
decoration owner and defaults to `1px`. A declaration on a descendant
replaces the inherited thickness for that descendant's emitted text runs;
omission preserves the inherited value. Existing specificity, stylesheet
order, and inline precedence remain unchanged.

Each selected underline, overline, and line-through keeps its existing line-y
origin and paints a contiguous vertical band of `thickness` rows toward
increasing y. Every row uses the existing integer decoration-style helper:
solid paints every x pixel, dashed uses `3 * thickness` painted pixels and
`2 * thickness` skipped pixels, and dotted uses `thickness` painted pixels and
`thickness` skipped pixels. The horizontal pattern is anchored at the emitted
run's x origin and restarts for each immutable text command. Thickness is
carried beside the existing glyph/decoration colors, style, and line flags in
`TextRun`.

The slice does not change text width, line formation, wrapping, alignment,
overflow geometry, line y origins, baseline or font metrics, decoration
offset, color, style, font, shaping, bidi, writing mode, accessibility
projections, hit testing, capture, opacity grouping, or source/semantic
order. The positive-y band is an experiment-local pixel rule; typographic
centering and browser decoration propagation are not claimed. The existing
integer-pixel, fixed-cell, horizontal-tb, fixture-first, default-off
`native-engine` boundary remains in force.

`auto`, `from-font`, zero, negative, percentage, non-pixel, fractional,
out-of-range, CSS-wide, unknown, empty, and other unsupported syntax remains
bounded typed diagnostics without raw stylesheet echo. The
`text-decoration` shorthand is not extended with thickness components by this
slice.

## Tradeoffs

- A `1px..=4px` bound keeps per-run work, surface writes, and overflow of the
  fixed-cell raster path explicit. It omits arbitrary lengths, font-derived
  thickness, fractional metrics, and CSS-wide reset semantics.
- Extending from each existing line y origin toward positive y is deterministic
  and leaves layout untouched, but can overlap nearby glyph rows. Baseline
  centering, offset negotiation, and browser font metrics remain outside the
  experiment.
- Scaling the existing dash/dot period by thickness gives one shared integer
  pattern owner and avoids a second style algorithm, but does not model
  browser-specific dash distribution or fragment continuity.
- Carrying thickness in `TextRun` keeps style, thickness, flags, colors, origin,
  clipping, scroll, opacity, capture, and raster replay on one immutable
  command. A separate decoration command would duplicate geometry.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

The implementation must provide parser, bounded-range, computed-style
inheritance, cascade, explicit override, declaration omission, and invalid
value diagnostic coverage. An integration fixture must prove one- through
four-pixel solid/dashed/dotted bands, x-origin anchoring, inherited and
overridden thickness, immutable display-command propagation, clipping, and
unchanged existing line geometry and 109 style/color/line consumers. Full
native integration, feature-library, strict lint, warning-denied rustdoc,
locked package/dependency, nightly/offline fuzz, documentation, static, and
formatting gates remain required. Every gate uses an isolated target with a
recorded purpose, and completed validation removes the exact regenerable
target after process and open-file checks.

Remote CI, browser parity, release, registry publication, complete CSS
decoration conformance, arbitrary thickness/offset/font metrics, and a third
crate remain outside this local task.

## Cleanup

Record the exact isolated target and report paths, sizes, process/open-file
checks, deletion counts, and post-removal filesystem state after certification.
Do not remove shared Cargo registries, toolchains, source, durable user data,
or other projects' non-regenerable artifacts.

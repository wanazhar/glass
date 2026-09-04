---
id: native-engine-109
scope: glass-browser/native-engine/text-decoration-style
status: design-ready
depends-on: [native-engine-108]
---

# Native bounded text-decoration-style

## Objective

Expose deterministic solid, dashed, and dotted presentation for the existing
fixed-cell decoration lines. The style must share the current immutable text
command, line origin, clipping, and software-raster owner without introducing
a second decoration geometry path.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-107.md`
- `docs/plan/tasks/native-engine-108.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the `text-decoration-style` longhand with one
case-insensitive token: `solid`, `dashed`, or `dotted`. The computed style is
bounded-inherited through the existing fixed-cell decoration owner and
defaults to `solid`. A declaration on a descendant replaces the inherited
style for that descendant's emitted text runs; omission preserves the existing
inherited style. Rule specificity, stylesheet order, and inline precedence
remain the existing cascade rules.

`solid` paints every pixel of each selected underline, overline, and
line-through run. `dashed` uses the existing integer pattern helper with a
one-pixel line width: three painted pixels followed by two skipped pixels.
`dotted` uses one painted pixel followed by one skipped pixel. Each pattern is
anchored at the emitted run's x origin and restarts for each immutable text
command. The style is carried beside the existing glyph/decoration colors and
line flags in `TextRun`; it changes only which existing line pixels are
emitted.

The slice does not change text width, line formation, wrapping, alignment,
overflow geometry, line y positions, color, thickness, offset, font metrics,
shaping, bidi, writing mode, accessibility projections, hit testing, capture,
opacity grouping, or source/semantic order. It remains integer-pixel,
fixed-cell, horizontal-tb, fixture-first, and default-off behind the existing
`native-engine` feature. `text-decoration` shorthand style components are not
added by this longhand slice.

`double`, `wavy`, CSS-wide keywords, unknown tokens, empty values, and other
unsupported syntax remain bounded typed diagnostics without raw stylesheet
echo. The bounded inherited style model is an experiment-local alias for the
existing line owner; full CSS decoration-origin propagation and longhand
conformance are not claimed.

## Tradeoffs

- Reusing `NativeBorderStyle` and its existing integer dash/dot helper keeps
  pattern semantics in one implementation and avoids a new renderer or
  dependency, but deliberately limits text styles to the three patterns that
  helper can represent.
- Carrying style in `TextRun` keeps glyphs, line flags, colors, origin, clip,
  scroll, opacity, capture, and raster replay on one immutable command. A
  separate decoration command would duplicate geometry and could drift.
- Anchoring each pattern at the run origin is deterministic and easy to test,
  but it does not model browser-wide decoration continuity across fragments,
  nodes, or line boxes.
- Treating the bounded style as inherited matches the current native line-state
  owner and makes descendant fixtures useful, at the cost of not claiming the
  full CSS decoration-origin/inheritance model.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

The implementation must provide parser, computed-style inheritance, cascade,
explicit-solid reset, invalid-value diagnostic, and declaration omission
coverage. An integration fixture must prove solid, dashed, and dotted pixels,
run-origin anchoring, inherited/overridden style, and the immutable display
command while preserving the existing line geometry and 108 longhand/color
consumers. Full native integration, feature-library, strict lint,
warning-denied rustdoc, locked package/dependency, nightly/offline fuzz,
documentation, static, and formatting gates remain required. Every gate uses
an isolated target with a recorded purpose, and completed validation removes
the exact regenerable target after process and open-file checks.

Remote CI, browser parity, release, registry publication, complete CSS
decoration conformance, wavy/double styles, and a third crate remain outside
this local task.

## Cleanup

Record the exact isolated target and report paths, sizes, process/open-file
checks, deletion counts, and post-removal filesystem state after certification.
Do not remove shared Cargo registries, toolchains, source, durable user data,
or other projects' non-regenerable artifacts.

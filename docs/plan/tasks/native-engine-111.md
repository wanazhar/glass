---
id: native-engine-111
scope: glass-browser/native-engine/text-underline-offset
status: design-ready
depends-on: [native-engine-110]
---

# Native bounded text-underline-offset

## Objective

Expose a bounded inherited signed-pixel `text-underline-offset` value for the
existing fixed-cell underline band. The value must travel through the current
immutable text command and adjust only the underline's raster y origin without
adding a second decoration geometry or layout path.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-110.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `text-underline-offset` with one
case-insensitive signed fixed-pixel value from `-4px` through `4px`, inclusive.
The computed value is inherited through the existing fixed-cell text
decoration owner and defaults to `0px`. A declaration on a descendant replaces
the inherited offset for that descendant's emitted text runs; omission
preserves the inherited value. Existing specificity, stylesheet order, and
inline precedence remain unchanged. `-0px` computes to zero. An explicit
positive offset moves the underline toward increasing y; an explicit negative
offset moves it toward decreasing y.

Only underline bands use the offset. Overline and line-through retain their
existing line origins. The underline's existing line origin is translated by
the signed offset before the 110 thickness band is painted; thickness still
extends each row toward increasing y. The existing solid/dashed/dotted helper
and its 110 thickness-scaled periods remain the only horizontal pattern owner,
anchored at each immutable text command's x origin. The signed offset is
carried beside the existing text colors, style, thickness, and line flags in
`TextRun`.

The slice does not change text width, line formation, wrapping, alignment,
overflow geometry, line-height, baseline or font metrics, glyph origin,
thickness, overline or line-through geometry, decoration color/style, font,
shaping, bidi, writing mode, accessibility projections, hit testing, capture,
opacity grouping, or source/semantic order. Negative offsets may overlap glyph
rows or other decorations; that is an intentional fixed-cell pixel rule, not
typographic offset negotiation. The existing integer-pixel, fixed-cell,
horizontal-tb, fixture-first, default-off `native-engine` boundary remains in
force.

`auto`, percentages, non-pixel, fractional, out-of-range, positive-sign,
CSS-wide, unknown, empty, and other unsupported syntax remains bounded typed
diagnostics without raw stylesheet echo. The `text-decoration` shorthand is
not extended with underline-offset components by this slice.

## Tradeoffs

- A signed `-4px..=4px` bound exposes both directions while bounding raster
  work and avoiding arbitrary or fractional metrics. It omits `auto`,
  percentages, and font-derived behavior.
- Applying the offset only to underline follows the property's bounded intent
  and leaves overline/line-through consumers stable, but does not model a
  general decoration-origin or all-line offset system.
- Reusing the existing line origin, thickness loop, style helper, clipping,
  scroll translation, opacity, and capture keeps one geometry/raster owner;
  negative values can intentionally overlap glyph pixels in this experiment.
- Carrying the offset in `TextRun` preserves immutable per-fragment state and
  restart semantics, but does not provide CSS fragment continuity across
  multiple commands.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

The implementation must provide parser, signed-range, computed-style
inheritance, cascade, explicit override, declaration omission, negative-zero,
and invalid-value diagnostic coverage. An integration fixture must prove
negative, zero, and positive underline placement, inherited and inline
override behavior, unchanged overline/line-through origins, thickness/style
composition, x-origin anchoring, immutable display-command propagation,
clipping, scroll translation, and unchanged layout geometry. Full native
integration, feature-library, strict lint, warning-denied rustdoc, locked
package/dependency, offline fuzz, documentation, static, and formatting gates
remain required. Every gate uses an isolated target with a recorded purpose,
and completed validation removes the exact regenerable target after process and
open-file checks.

Remote CI, browser parity, release, registry publication, complete CSS
decoration conformance, `auto`/percentage/font metrics, decoration-origin
propagation, fragment continuity, and a third crate remain outside this local
task.

## Cleanup

Record the exact isolated target and report paths, sizes, process/open-file
checks, deletion counts, and post-removal filesystem state after certification.
Do not remove shared Cargo registries, toolchains, source, durable user data,
or other projects' non-regenerable artifacts.

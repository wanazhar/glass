---
id: native-engine-112
scope: glass-browser/native-engine/text-decoration-style-double
status: design-ready
depends-on: [native-engine-111]
---

# Native bounded double text-decoration style

## Objective

Expose the bounded inherited `text-decoration-style:double` value through the
existing fixed-cell decoration path. The value must remain distinct from
border styling, travel through the current immutable text command, and paint
two deterministic solid bands without adding a second layout or display-list
owner.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-111.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the case-insensitive inherited
`text-decoration-style:double` keyword alongside the already supported
`solid|dashed|dotted` values. Omission continues to compute to `solid`, and
existing specificity, declaration order, inline precedence, and inheritance
remain unchanged. The text-decoration style is represented by a dedicated
`NativeTextDecorationStyle`; the existing `NativeBorderStyle` and border CSS
grammar do not gain `double` as a side effect.

For every selected decoration line, `double` paints two solid horizontal bands.
The first band begins at the existing line origin and retains the resolved
`text-decoration-thickness`; the second begins one transparent pixel after
the first band and has the same thickness. Thus the bounded fixed-cell
vertical footprint is `2 * thickness + 1` pixels, with the existing
`1px..=4px` thickness bound and normal surface/clip clipping. The underline
origin is still translated by the existing signed `-4px..=4px`
`text-underline-offset` before both bands are placed. Overline and
line-through retain their existing origins and do not consume underline
offset state.

The two bands are solid across the immutable text run width and remain
anchored at its x origin. Dashed and dotted styles retain their 110 horizontal
pattern helper and one-band-per-thickness behavior. The resolved dedicated
style travels beside decoration color, line flags, thickness, and underline
offset in the same immutable `TextRun`; clipping, root scroll translation,
opacity replay, capture, hit testing, semantic projection, and source order
continue to reuse their existing consumers.

The slice does not change text width, line formation, wrapping, alignment,
overflow geometry, line-height, baseline or font metrics, glyph origin,
decoration color, thickness, underline offset, font, shaping, bidi, writing
mode, accessibility projections, hit testing, capture, opacity grouping, or
source/semantic order. It does not add decoration-origin propagation,
fragment continuity, CSS centering, or browser-wide text conformance. The
existing integer-pixel, fixed-cell, horizontal-tb, fixture-first,
default-off `native-engine` boundary remains in force.

`wavy`, CSS-wide keywords, unknown, empty, and other unsupported syntax
remains bounded typed diagnostics without raw stylesheet echo. The
`text-decoration` shorthand is not extended with style components by this
slice.

## Tradeoffs

- A dedicated text-decoration style type prevents `double` from silently
  becoming a border style and makes the boundary explicit, at the cost of
  touching the public native display-command style type.
- Retaining the full resolved thickness for each solid band makes 110's
  thickness meaning stable and deterministic, but the total double footprint
  is `2 * thickness + 1` rather than a browser font-metric fit.
- The one-pixel separation and positive-y band placement are deliberately
  simple fixed-cell rules. They may overlap glyphs or adjacent decorations,
  while clipping and the existing signed underline offset remain the only
  bounds owners.
- Reusing the existing immutable command, line origins, x anchoring, clip,
  scroll, opacity, capture, and software replay keeps one artifact pipeline,
  but does not provide real typographic decoration geometry or fragment
  continuity.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

The implementation must provide parser/cascade coverage for case-insensitive
`double`, default solid behavior, inheritance, explicit override, omission,
inline precedence, and unsupported `wavy` diagnostics. Native integration
must prove two solid bands for underline, overline, and line-through, their
composition with thickness and underline offset, unchanged line origins,
x-origin anchoring, clipping, scroll translation, immutable command
propagation, and unchanged layout geometry. Existing solid/dashed/dotted
regressions must remain green. Full native integration, feature-library,
strict lint, warning-denied rustdoc, locked package/dependency, offline fuzz,
documentation, static, security, and formatting gates remain required.

Every local gate uses an isolated or intentionally shared target with a
recorded purpose. Completed validation removes exact regenerable output only
after active-writer and open-file checks. Remote CI, browser parity, release,
registry publication, and a third crate are not claimed by this local task.

## Cleanup

Record the exact isolated target and report paths, sizes, process/open-file
checks, deletion counts, and post-removal filesystem state after
certification. Do not remove shared Cargo registries, toolchains, source,
durable user data, or other projects' non-regenerable artifacts.

---
id: native-engine-113
scope: glass-browser/native-engine/text-decoration-style-wavy
status: design-ready
depends-on: [native-engine-112]
---

# Native bounded wavy text-decoration style

## Objective

Expose the bounded inherited `text-decoration-style:wavy` value through the
existing fixed-cell decoration path. The value must remain distinct from
border styling, travel through the current immutable text command, and paint a
deterministic fixed-pixel wave without adding a second layout or display-list
owner.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-112.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the case-insensitive inherited
`text-decoration-style:wavy` keyword alongside the supported
`solid|dashed|dotted|double` values. Omission continues to compute to `solid`,
and existing specificity, declaration order, inline precedence, and inheritance
remain unchanged. The text-decoration style is represented by the existing
dedicated `NativeTextDecorationStyle`; `NativeBorderStyle` and border CSS
grammar do not gain `wavy` as a side effect.

For every selected decoration line, `wavy` paints a continuous fixed-pixel
wave across the immutable text run. Its deterministic horizontal period is
eight pixels and its vertical phase is the repeating sequence
`[0, 1, 2, 1, 0, -1, -2, -1]` pixels. At each x column, the resolved
`text-decoration-thickness` paints that many consecutive pixels beginning at
the phase-adjusted line origin. The nominal line origin is phase zero; the
wave may therefore extend above or below that origin. The existing
`1px..=4px` thickness bound remains in force.

The wave starts at each emitted run's x origin; it does not continue phase
across fragments. Underline phase zero is translated by the existing signed
`-4px..=4px` `text-underline-offset` before the wave is sampled. Overline and
line-through retain their existing origins and do not consume underline
offset state. Decoration color, clipping, root scroll translation, opacity
replay, capture, hit testing, semantic projection, and source order continue
to use their existing consumers.

The slice does not change text width, line formation, wrapping, alignment,
overflow geometry, line-height, baseline or font metrics, glyph origin,
decoration color, thickness, underline offset, font, shaping, bidi, writing
mode, accessibility projections, hit testing, capture, opacity grouping, or
source/semantic order. It does not add decoration-origin propagation,
fragment continuity, CSS metric centering, antialiasing, or browser-wide text
conformance. The existing integer-pixel, fixed-cell, horizontal-tb,
fixture-first, default-off `native-engine` boundary remains in force.

`double` retains its two-band behavior, and `solid`, `dashed`, and `dotted`
retain their existing behavior. The `text-decoration` shorthand is not
extended with style components by this slice.

## Tradeoffs

- A fixed eight-pixel phase and two-pixel amplitude make wavy output stable,
  cheap, and testable across platforms, but do not model font metrics, CSS
  stroke centering, or browser wave geometry.
- Applying the resolved thickness at every x column preserves the 110
  thickness contract, but thick waves can merge neighboring phases and occupy
  a larger vertical footprint than a one-pixel wave.
- Anchoring phase at every immutable run keeps the existing artifact ownership
  and avoids hidden cross-fragment state, but visible waves can restart at
  fragment boundaries.
- Reusing the existing line origin, offset, clip, scroll, opacity, capture,
  hit-test, semantic, and source-order paths keeps the change small and
  auditable, while leaving decoration-origin propagation, vertical writing,
  shaping, and full CSS conformance explicitly out of scope.

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
`wavy`, default solid behavior, inheritance, explicit override, omission,
inline precedence, and unsupported syntax diagnostics. Native integration must
prove the exact eight-pixel phase, thickness-scaled vertical strokes, run-origin
phase reset, all three decoration lines, composition with underline offset,
unchanged line origins, clipping, scroll translation, immutable command
propagation, and unchanged layout geometry. Existing solid/dashed/dotted and
double regressions must remain green. Full native integration, feature-library,
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

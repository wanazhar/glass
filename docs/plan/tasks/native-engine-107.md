---
id: native-engine-107
scope: glass-browser/native-engine/text-decoration-color
status: design-ready
depends-on: [native-engine-106]
---

# Native bounded text-decoration color

## Objective

Extend the completed fixed-cell decoration-line owner with an explicit
`text-decoration-color` value. Glyph color and decoration color must remain
independent while one immutable text command continues to carry the complete
paint input through clipping, scrolling, opacity replay, capture, and the
software rasterizer.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-105.md`
- `docs/plan/tasks/native-engine-106.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `text-decoration-color` as a local,
non-inherited color declaration. It reuses the existing bounded `NativeColor`
grammar: `black`, `white`, `red`, `green`, `blue`, `transparent`, three-, six-,
and eight-digit hexadecimal colors, and the existing comma-form `rgb(...)` and
`rgba(...)` forms. Matching of named colors and the property name is
case-insensitive through the existing parser. An omitted declaration resolves
to the emitted text run's resolved `color`; if that is absent, the existing
black fallback is used. `currentColor`, CSS-wide keywords, gradients, system
colors, and other color syntaxes remain typed unsupported-value diagnostics
without raw stylesheet echo.

The computed style retains the optional explicit decoration color separately
from inherited text color. For every emitted text run, the display list carries
both the glyph color and the resolved decoration color. Enabled
`underline`, `overline`, and `line-through` pixels blend with the decoration
color; glyph pixels blend with the text color. A transparent decoration color
is valid and leaves the line pixels unchanged. The color does not change text
width, line formation, wrapping, alignment, overflow geometry, hit testing,
semantic/source order, accessibility projections, scroll offsets, opacity
group boundaries, capture dimensions, or clipping ownership.

The local model deliberately resolves an omitted decoration color at each
emitted text run. Therefore an inherited line can use a descendant's resolved
text color when no explicit local decoration color is present. Standard
decoration-origin propagation and `text-decoration-color: inherit` semantics
are not claimed by this bounded contract.

The slice remains restricted to the current integer-pixel, fixed-cell,
horizontal-tb implementation. `text-decoration-line` longhand semantics,
shorthand color components, decoration style/thickness/offset, font-aware
metrics, shaping, bidi, vertical writing, color spaces, animations, and
browser-wide text conformance remain outside the contract.

## Tradeoffs

- Carrying a second color in the existing immutable `TextRun` command keeps
  glyphs and decoration lines tied to one text origin, width, clip, scroll
  translation, opacity group, and capture path. A separate decoration command
  would duplicate geometry and could diverge from the text run.
- Reusing `parse_color` provides useful deterministic palette and alpha
  coverage without adding a color dependency or a second color grammar.
- Keeping the declaration local matches the computed-style boundary used by
  the experiment, but it knowingly differs from full browser decoration
  propagation when a parent supplies the line and a child changes color.
- Explicit `transparent` is retained rather than treated as absent, so
  authors can suppress decoration pixels without clearing inherited line bits.
- This is the first native slice that intentionally changes the internal
  display-command shape. The change is bounded, feature-gated, and covered by
  all display-list/raster consumers; it does not create a crate, dependency,
  renderer, or second geometry owner.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

The implementation must provide focused parser/cascade coverage for explicit,
fallback, alpha, transparent, precedence, and unsupported values; an
integration test that distinguishes glyph and decoration pixels; and full
native integration coverage proving that existing layout, clip, scroll,
opacity, capture, hit-test, semantic, and source-order consumers remain
stable. The full feature-enabled library, strict Clippy, warning-denied
rustdoc, locked package/dependency, nightly/offline fuzz, static documentation
validators, fresh source-built documentation coverage, and formatting gates
remain required. Every gate uses an isolated target with a recorded purpose;
completed validation removes the exact regenerable target after process and
open-file checks.

Remote CI, browser parity, release, registry publication, and a complete CSS
color-conformance claim remain outside this local task.

## Cleanup

Record the exact isolated target and report paths, their sizes, the process and
open-file checks, and post-removal filesystem state at completion. Do not
remove shared Cargo registries, toolchains, source, durable user data, or
other projects' non-regenerable artifacts.

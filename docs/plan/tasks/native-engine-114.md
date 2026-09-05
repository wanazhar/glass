---
id: native-engine-114
scope: glass-browser/native-engine/text-decoration-skip-ink
status: implementation-in-progress
depends-on: [native-engine-113]
---

# Native bounded text-decoration skip-ink

## Objective

Expose the bounded inherited `text-decoration-skip-ink:auto|none` values
through the existing fixed-cell decoration path. `auto` must make decoration
replay glyph-aware without adding a second layout, display-list, or semantic
owner; `none` must preserve the existing decoration pixels.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-113.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the case-insensitive inherited
`text-decoration-skip-ink:auto|none` values. Omission computes to `auto`.
Unsupported `all`, CSS-wide, unknown, empty, and other out-of-contract values
remain typed diagnostics without raw stylesheet echo. The value is represented
by a dedicated `NativeTextDecorationSkipInk`; border styling and all existing
`text-decoration-style` values remain unchanged.

With `none`, every selected decoration pixel follows the existing line style,
thickness, offset, clipping, scroll, opacity, capture, and command replay
behavior. With `auto`, an individual underline or overline pixel is suppressed
only when its fixed-cell raster coordinate intersects an ink pixel emitted by
the same immutable `TextRun`. Line-through remains unaffected. The comparison
uses the existing bold and italic fixed-cell glyph rasterization, so the skip
decision follows the actual bounded glyph mask rather than a second geometry
approximation.

The behavior is intentionally observable for the existing wavy style, whose
phase can cross the seven-row glyph mask; ordinary line origins that do not
intersect glyph rows remain unchanged. Decoration color, style, thickness,
underline offset, line origins, text width, line formation, wrapping,
alignment, overflow geometry, clipping, scrolling, opacity, capture, hit
testing, semantic projection, and source order continue to use their existing
consumers. The value is inherited through the existing DOM style chain and
carried in the immutable text command.

The slice remains integer-pixel, fixed-cell, horizontal-tb, fixture-first,
default-off, and local-only. It does not add font metrics, shaping, bidi,
writing modes, decoration-origin propagation, fragment continuity, CSS-wide
inheritance semantics, anti-aliased ink, or browser-wide text conformance.

## Tradeoffs

- A glyph-mask intersection is deterministic and cheap to replay, but it is
  not a font-metric or shaped-glyph implementation of CSS skip-ink.
- Suppressing only matching same-run pixels avoids a second geometry owner and
  keeps source/semantic order intact, but gaps do not continue across emitted
  run boundaries.
- Supporting only `auto|none` keeps the public grammar auditable; `all` and
  other CSS values remain explicit diagnostics until a bounded contract exists.
- Applying the decision to underline and overline while leaving line-through
  unchanged matches the intended bounded property surface, but does not claim
  complete CSS decoration behavior.

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
`auto|none`, defaulting, inheritance, explicit override, inline precedence,
omission, and unsupported-value diagnostics. Native integration must prove
that `auto` skips matching underline and overline glyph pixels, `none`
preserves them, line-through is unchanged, bold/italic masks are respected,
and wavy, thickness, offset, clipping, scroll, opacity, capture, immutable
command propagation, and layout geometry remain unchanged. Existing solid,
dashed, dotted, double, and wavy regressions must remain green. Full native,
feature-library, strict lint, warning-denied rustdoc, locked package/
dependency, offline fuzz, documentation, static, security, and formatting
gates remain required.

Every local gate uses an isolated or intentionally shared target with a
recorded purpose. Completed validation removes exact regenerable output only
after active-writer and open-file checks. Remote CI, browser parity, release,
registry publication, and a third crate are not claimed by this local task.

## Cleanup

Record the exact isolated target and report paths, sizes, process/open-file
checks, deletion counts, and post-removal filesystem state after
certification. Do not remove shared Cargo registries, toolchains, source,
durable user data, or other projects' non-regenerable artifacts.

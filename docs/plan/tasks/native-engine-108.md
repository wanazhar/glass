---
id: native-engine-108
scope: glass-browser/native-engine/text-decoration-line
status: design-ready
depends-on: [native-engine-107]
---

# Native bounded text-decoration-line longhand

## Objective

Expose the existing fixed-cell decoration-line bitset through the
`text-decoration-line` longhand. The longhand must share the current
immutable text command, fixed-pixel decoration owner, and artifact consumers
without introducing a second line-state or geometry path.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-106.md`
- `docs/plan/tasks/native-engine-107.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `text-decoration-line` with the same bounded
line tokens already accepted by the `text-decoration` shorthand:
`none`, `underline`, `overline`, and `line-through`. The three line tokens may
appear once each in any order; `none` is valid only by itself. Matching is
case-insensitive through the existing parser. The longhand maps directly to
the existing `TextDecorationValue` bitset.

Within one declaration list, `text-decoration` and
`text-decoration-line` share one declaration-order-aware line-state slot, so
the last supported declaration wins. Across stylesheet rules they retain the
existing specificity, rule-order, and inline precedence. An explicit `none`
clears the current inherited line state. When the longhand is omitted, the
existing inherited fixed-cell line state remains unchanged. This is a bounded
alias inside the current inherited decoration owner; full CSS distinctions
between computed longhand inheritance and decoration propagation are not
claimed.

The selected bits continue through the existing immutable `TextRun`, clipping,
root scrolling, opacity replay, capture, hit testing, semantic/source order,
and software raster paths. The longhand changes only which of the existing
underline, overline, and line-through pixels are emitted. It does not change
text width, line formation, wrapping, alignment, overflow geometry, color,
thickness, style, offset, font metrics, shaping, bidi, writing mode, or
accessibility projections.

CSS-wide keywords, unknown tokens, duplicate line tokens, mixed `none` values,
`text-decoration-style`, `text-decoration-thickness`, shorthand color
components, and other unsupported syntax remain bounded typed diagnostics
without raw stylesheet echo. The bounded implementation remains integer-pixel,
fixed-cell, horizontal-tb, fixture-first, and default-off behind the existing
`native-engine` feature.

## Tradeoffs

- Normalizing both declarations into the current line bitset keeps one source
  of truth for line flags and makes declaration order observable without
  duplicating cascade or paint state.
- Reusing the existing parser provides combinations and rejection behavior
  already covered by the shorthand, but intentionally does not add the full
  CSS longhand inheritance/propagation model.
- Keeping the display command and raster geometry unchanged limits regression
  surface and preserves all 107 color behavior, at the cost of not modeling
  independent decoration-origin propagation.
- An explicit `none` remains meaningful and clears inherited bits, while an
  omitted longhand preserves the existing inherited state.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

The implementation must provide parser and declaration-order coverage for the
longhand, shorthand interaction, combinations, explicit `none`, invalid
values, and diagnostic redaction. An integration fixture must prove longhand
line flags and pixels while preserving the existing shared geometry and
artifact consumers. Full native integration, feature-library, strict lint,
warning-denied rustdoc, locked package/dependency, nightly/offline fuzz,
documentation, static, and formatting gates remain required. Every gate uses
an isolated target with a recorded purpose, and completed validation removes
the exact regenerable target after process and open-file checks.

Remote CI, browser parity, release, registry publication, complete CSS
decoration conformance, and a third crate remain outside this local task.

## Cleanup

Record the exact isolated target and report paths, sizes, process/open-file
checks, deletion counts, and post-removal filesystem state after certification.
Do not remove shared Cargo registries, toolchains, source, durable user data,
or other projects' non-regenerable artifacts.

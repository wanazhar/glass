---
id: native-engine-086
scope: glass-browser/native-engine/align-self-stretch
status: active
depends-on: [native-engine-085]
---

# Native bounded align-self stretch

## Objective

Extend the existing bounded non-inherited `align-self` contract with
`stretch` for eligible direct flex items. An auto-height item must fill the
existing flex-line cross size through the same layout box, descendant,
display-list, raster, overflow, projection, hit-test, scrolling, and capture
owners. The slice must not create a second cross-axis geometry representation.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `align-self:stretch` in addition to the
completed `auto`, `flex-start`, `center`, and `flex-end` values. The property
remains non-inherited and scoped to eligible direct element children of the
existing fixed-width row or row-reverse flex layout.

For an item whose computed `height` is omitted, `stretch` resolves its used
outer height to the existing line cross size minus its vertical margins. The
result reuses the current bounded box model, including physical padding and
border insets plus bounded pixel `min-height` and `max-height` constraints. It
never shrinks an item below the natural height already produced by the shared
layout pass. In a single non-wrapping row, the existing explicit parent
content height is the line cross size; in wrapped rows, the existing formed
line height remains authoritative.

For an item with an explicit bounded `height`, `stretch` does not rewrite the
declared size. The item uses the bounded flex-start placement fallback, rather
than inheriting the parent `align-items` offset. This keeps the slice
deterministic without adding an auto-size or margin-resolution algorithm.

The expanded value preserves existing specificity, source order, inline
precedence, and invalid-declaration retention. A valid later longhand wins at
its existing cascade position; an invalid later declaration does not erase an
earlier valid `align-self` value. Unsupported CSS-wide, baseline, normal,
logical start/end, safe/unsafe, multi-token, and other unsupported forms
remain typed diagnostics.

The parent `align-items` grammar remains bounded to its existing
`flex-start|center|flex-end` values; this slice does not add
`align-items:stretch`, auto margins, column directions, intrinsic or
fractional sizing, or general Flexbox conformance. The stretched root box and
its complete descendant artifact range must remain consistent across layout,
line overflow, display-list paint, software rasterization, viewport
projection, hit testing, scrolling, capture, and semantic/source order.

## Tradeoffs

- Stretch is implemented as a used-size adjustment after the existing natural
  child layout and line-size calculation. That preserves one line owner and
  keeps descendant flow deterministic, but it intentionally does not model
  every CSS auto-margin, baseline, min-content, or intrinsic-sizing rule.
- Explicit heights stay fixed and top-aligned under the bounded fallback. This
  avoids silently changing an author-declared size, at the cost of not claiming
  full CSS used-value behavior for every non-auto cross size.
- Existing physical padding, borders, and bounded min/max heights constrain
  the stretched outer box. Logical properties, percentages, fractional
  lengths, and writing-mode semantics remain outside the native boundary.
- No new dependency or geometry owner is introduced. The native backend stays
  default-off inside `glass-browser`, and Chromium/CDP remains the production
  runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Implementation, focused parser/cascade and shared-layout tests, full native
suites, strict feature/documentation gates, issue #40 status, and exact
regenerable-target cleanup will be recorded here when the slice closes. Remote
CI is not claimed until the local branch is pushed.

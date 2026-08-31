---
id: native-engine-052
scope: glass-browser/native-engine/text-alignment
status: in-progress
depends-on: [native-engine-051]
---

# Native bounded inherited text alignment

## Objective

Add a bounded physical `text-align` presentation slice to the existing
fixed-cell normal-flow owner. Supported text and inline content should align
as one line within the containing content width while preserving the current
layout, paint, hit-test, scroll, and raster ownership.

## Contract

The native CSS grammar accepts only `left`, `center`, and `right` for
`text-align`. The property is inherited through the existing DOM style walk;
the initial value is `left`. Invalid, logical, direction-dependent, and
justification values are diagnosed as unsupported and do not change the
cascade.

Each bounded flow container aligns each completed fixed-cell line within its
available content width. `left` adds no offset, `center` adds half of the
remaining width rounded down, and `right` adds all remaining width. The line
alignment pass shifts every artifact created by a line item, including direct
text fragments and supported inline element boxes with their descendants, so
layout boxes, text origins, display-list paint, software rasterization, and
point hit testing retain one coordinate owner. Line breaking, line height,
box dimensions, root scrolling, and opacity-group boundaries do not change.

Preformatted or `nowrap` lines that exceed the available width receive no
negative alignment offset and retain their existing bounded overflow behavior.
`display:contents` remains flow-transparent and uses the containing flow's
alignment. This slice does not add `justify`, `start`/`end`, direction or
writing-mode resolution, vertical alignment, browser font metrics, bidi,
letter/word spacing, or general inline-formatting parity.

The line-item bookkeeping is bounded by the existing DOM/layout/display-list
limits and does not add a dependency or a second geometry owner.

## Tradeoffs

- Shifting complete line-item ranges keeps inline boxes and text visually and
  interactively together, but requires a bounded post-placement adjustment at
  each line flush.
- Fixed-cell remaining-width arithmetic is deterministic and dependency-free,
  but does not model font metrics, bidi, justification, or logical writing
  directions.
- Alignment stays inside the existing content width, so it cannot create a
  new scroll extent; intentionally wide `pre`/`nowrap` lines remain left
  anchored when no free width exists.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parsing, invalid-value diagnostics, selector/inline cascade, and inherited
  computed alignment are covered by unit tests;
- left/center/right alignment covers multiple lines, direct text, supported
  inline boxes, and `display:contents` flow;
- layout boxes, text origins, display-list commands, software pixels, point
  hit testing, scrolling, and pre/nowrap overflow retain one deterministic
  aligned coordinate result;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

Implementation is not yet complete. This file records the dependency-ordered
contract before source changes; validation and commit evidence will be added
after the focused implementation pass.

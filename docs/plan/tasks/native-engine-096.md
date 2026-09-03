---
id: native-engine-096
scope: glass-browser/native-engine/flex-direction-column-wrap-reverse
status: ready
depends-on: [native-engine-095]
---

# Native bounded column wrap-reverse

## Objective

Extend the bounded column Flexbox owner from `flex-wrap:wrap` to
`flex-wrap:wrap-reverse` for eligible fixed-height `column` and
`column-reverse` containers. Reuse 095 line formation, per-line sizing,
justification, cross-line distribution, and shared artifact ownership while
reflecting the horizontal cross axis at the physical cross-end.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Flexible Box Layout Module Level 1: flex-wrap](https://www.w3.org/TR/css-flexbox-1/#flex-wrap)
- [CSS Flexible Box Layout Module Level 1: multi-line flex containers](https://www.w3.org/TR/css-flexbox-1/#multi-line-flex-container)
- [CSS Flexible Box Layout Module Level 1: align-content](https://www.w3.org/TR/css-flexbox-1/#align-content-property)
- [CSS Flexible Box Layout Module Level 1: flex-direction](https://www.w3.org/TR/css-flexbox-1/#flex-direction)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The existing computed `flex-direction:column|column-reverse` and
`flex-flow:column wrap-reverse` values become layout-capable for one bounded
context:

- the container is `display:flex`, has a finite explicit content height and a
  finite available content width, and uses `flex-wrap:wrap-reverse`;
- every visible direct element child has a bounded explicit height or a
  bounded pixel `flex-basis`; direct text, hidden/non-rendered children, and
  unsupported node shapes retain the established fallback boundary;
- visual items are sorted by `(order, source_index)` before line formation;
  DOM, semantic, keyboard, and source order remain unchanged;
- line formation is identical to 095: order-sorted items form vertical lines
  against the fixed content height, with `row-gap` inside a line and an item
  that cannot fit an empty line still retained;
- each line keeps the 095 fixed content height, per-line integer flex
  grow/shrink/basis, `justify-content`, and complete descendant/artifact
  ownership. `column` places the main-axis items top-to-bottom and
  `column-reverse` places them bottom-to-top;
- the formed line sequence is reflected across the horizontal content box:
  the first source-order line is placed from the physical cross-end and later
  lines proceed toward the physical cross-start. `column-gap` remains the
  bounded gap between adjacent horizontal line boxes;
- every existing `align-content` value supported by 095
  (`flex-start|center|flex-end|space-between|space-around|space-evenly|
  stretch|normal`) is reflected with the line boxes, including its leading,
  inter-line, trailing, and deterministic integer remainder behavior;
- `align-items` and `align-self` cross-axis placement is reflected with the
  line: `flex-start`/stretch/normal use the line's cross-start, `flex-end`
  uses its cross-end, and center remains centered. Explicit item widths stay
  authoritative; auto-width stretch uses the resolved line width;
- line and item coordinates are finalized before layout artifacts are emitted.
  Complete descendant ranges and shared document-space geometry feed display
  lists, software rasterization, viewport projection, root overflow, scrolling,
  point hit testing, capture, and semantic/source-order consumers;
- unsupported or ineligible column layouts retain the established normal-flow
  fallback. `flex-wrap:wrap` continues to use native-engine-095.

## Tradeoffs

- Reflecting the completed 095 line geometry at the cross-axis owner keeps
  `align-content`, gaps, and line widths consistent without a second artifact
  translation pass. It makes the cross-axis reversal explicit in the layout
  contract rather than treating it as a row-only behavior.
- Reversing only physical line placement and cross-axis item alignment preserves
  source and semantic order while matching the intended visual meaning of
  `wrap-reverse`; `column-reverse` remains an independent main-axis concern.
- The slice continues to use bounded integer line widths and fixed content
  dimensions. It does not import intrinsic sizing, logical writing modes, or
  an unbounded CSS alignment model just to cover a wider browser surface.
- Keeping 095's eligible-child gate avoids mixing unsupported anonymous text,
  auto-height columns, and intrinsic sizing into the cross-axis reflection;
  those cases remain diagnosable through the established fallback behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Implementation and documentation closeout are pending. The design checkpoint
is this task's initial ready commit; its hash will be recorded before the
implementation checkpoint.

Required evidence:

- focused column `wrap-reverse` integration for both `column` and
  `column-reverse`, including `align-content`, `align-items`/`align-self`,
  gaps, descendants, paint, and hit testing;
- explicit fallback coverage for an ineligible/unsupported column shape;
- full native integration and feature-enabled library coverage with the
  documented non-default test stack where required;
- lockfile-pinned Clippy, rustdoc, package, binary, fuzz, documentation, and
  relevant static validators;
- exact temporary-target and report cleanup after process/open-file checks.

Remote CI remains pending while `main` is local-only. No push, tag, release,
registry publication, or browser-parity certification is part of this slice.


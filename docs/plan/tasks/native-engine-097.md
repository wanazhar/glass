---
id: native-engine-097
scope: glass-browser/native-engine/flex-auto-margins
status: ready
depends-on: [native-engine-096]
---

# Native bounded flex auto margins

## Objective

Make bounded `margin:auto` values participate in the existing no-wrap flex
line owners. Auto margins must absorb positive free space before the existing
`justify-content` and `align-items` decisions, while preserving one geometry
owner for row, row-reverse, column, and column-reverse layouts and every
descendant/artifact consumer.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Flexible Box Layout Module Level 1: flex item margins and paddings](https://www.w3.org/TR/css-flexbox-1/#item-margins)
- [CSS Flexible Box Layout Module Level 1: aligning with auto margins](https://www.w3.org/TR/css-flexbox-1/#auto-margins)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar and cascade accept `auto` in `margin`,
`margin-top`, `margin-right`, `margin-bottom`, and `margin-left`. Existing
non-negative fixed-pixel lengths remain numeric margins; each auto edge is
retained as a distinct computed state so declaration precedence and layout can
observe it. Auto margins remain non-inherited and are treated as zero by the
existing non-flex flow owner.

Layout consumes auto margins only for one bounded no-wrap context:

- the container is `display:flex`, uses `flex-wrap:nowrap`, and is eligible for
  the existing row/row-reverse or fixed-height column/column-reverse owner;
- visible direct element children retain the current eligibility and sizing
  rules. Row items use the existing bounded width path; column items use the
  existing bounded explicit-height or pixel-`flex-basis` path. Hidden,
  unsupported, anonymous-text, and fallback children remain outside this
  owner;
- auto margins are zero while the existing flex grow, shrink, basis, gap, and
  line-size calculations run. Positive remaining main-axis space after those
  calculations is divided equally across the auto main-axis edges on the line,
  using deterministic integer quotient-plus-prefix-remainder allocation;
- when one or more main-axis auto margins receive space, the existing
  `justify-content` distribution contributes no additional main-axis offset.
  With no positive remaining space, auto margins resolve to zero and the
  existing justify/overflow behavior is retained. Reverse directions apply the
  resolved margin on the corresponding physical edge without changing source,
  semantic, or keyboard order;
- for a finite single-line cross size, positive cross-axis remainder is divided
  equally across an item's auto cross-axis edges before `align-items` or
  `align-self`. Cross-axis auto margins therefore suppress that item's normal
  cross-axis alignment; when the item overflows, its auto margins resolve to
  zero and the existing bounded alignment/overflow path is used;
- each resolved auto-margin edge is included in item outer geometry, gaps,
  line sizing, root overflow, scrolling, point hit testing, display-list
  translation, software rasterization, viewport projection, capture, and
  semantic/source-order consumers. No second layout or artifact coordinate
  owner is introduced;
- `flex-wrap:wrap|wrap-reverse`, auto-height columns, intrinsic or percentage
  sizes, automatic margin behavior in normal flow, logical writing modes,
  baseline alignment, grid, and browser-wide Flexbox remain outside this
  bounded slice and retain the established fallback boundary.

## Tradeoffs

- Keeping auto edges separate from numeric box edges preserves the existing
  box-model API and makes non-flex flow fail closed, but it does not claim
  block-flow auto-margin centering or margin collapsing.
- Resolving only positive integer free space matches the important flex
  alignment behavior while keeping rounding reproducible. Fractional lengths,
  percentages, negative margins, and full flexible-length freezing remain
  outside the contract.
- Applying the rule to no-wrap lines first avoids inventing per-line auto
  margin state for wrapped rows and columns. The 095/096 line formation and
  cross-line distribution remain unchanged until a later dependency-ordered
  slice explicitly owns them.
- Cross-axis auto margins are limited to finite existing line sizes. This
  avoids treating the current auto-height fallback as a complete cross-size
  algorithm while still covering the common toolbar and column-footer pattern.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The certification gate will include focused parser/cascade tests, row and
column auto-margin placement for forward and reverse directions, precedence
and overflow tests, complete descendant/artifact checks, explicit wrapped and
intrinsic fallback coverage, the full native integration and feature-enabled
library suites, strict all-feature and no-default-feature Clippy,
warning-denied workspace rustdoc, locked `glass-dev` binary compilation,
static documentation/release validators, live documentation coverage with
isolated temporary binaries, and exact isolated-target cleanup.

Remote CI remains pending because `main` is local-only and has not been pushed.
No new crate, dependency, runtime, network, JavaScript, storage, artifact
pipeline, browser-parity certification, or release publication is part of
this slice.

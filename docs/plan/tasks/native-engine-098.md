---
id: native-engine-098
scope: glass-browser/native-engine/flex-wrapped-auto-margins
status: ready
depends-on: [native-engine-097]
---

# Native bounded wrapped flex auto margins

## Objective

Extend the existing wrapped flex line owners so eligible `margin:auto` edges
resolve independently inside each formed row or column line. Preserve the
single layout/artifact owner across forward and reverse directions,
`wrap-reverse`, line distribution, and every descendant consumer without
expanding into general Flexbox sizing.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-097.md`
- [CSS Flexible Box Layout Module Level 1: flex item margins and paddings](https://www.w3.org/TR/css-flexbox-1/#item-margins)
- [CSS Flexible Box Layout Module Level 1: aligning with auto margins](https://www.w3.org/TR/css-flexbox-1/#auto-margins)
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

This slice consumes the auto-edge state and deterministic share helper from
097. It adds no CSS grammar, dependency, crate, runtime, network, JavaScript,
storage, or artifact-pipeline surface.

Layout consumes wrapped auto margins only for the existing eligible flex
owners:

- the container is `display:flex` with `flex-wrap:wrap|wrap-reverse`; row and
  row-reverse use the existing bounded row owner, while column and
  column-reverse require the existing finite explicit-height column owner;
- visible direct element children retain the current eligibility, integer
  fixed-pixel/intrinsic item measurement, order sorting, gap, flex grow/shrink,
  basis, and min/max rules. Unsupported, hidden, anonymous-text, and fallback
  children retain the established normal-flow boundary;
- auto margins are zero while line membership, line cross-size formation, and
  the existing per-line flex sizing run. Auto edges therefore do not consume
  wrap capacity or inflate a line's provisional cross size;
- after each line has its final bounded size, including the existing
  `align-content` or stretch adjustments, positive main-axis remainder is
  divided equally across that line's auto main-axis edges. A line whose auto
  margins consume positive remainder receives no additional
  `justify-content` main-axis offset. With no positive remainder, the existing
  justify and overflow behavior remains unchanged;
- positive cross-axis remainder is divided equally across each item's auto
  cross-axis edges within its final line. Cross-axis auto margins suppress that
  item's normal `align-items`/`align-self` placement. If the item overflows its
  line, the auto margins resolve to zero and the existing bounded alignment and
  overflow path is retained;
- row and column reverse directions preserve source, semantic, and keyboard
  order while applying resolved margins to their physical edges. `wrap-reverse`
  reflects line placement through the existing physical cross-end mapping; it
  does not create a second line or margin coordinate system;
- every line-local resolved edge remains part of item outer geometry, gaps,
  line extent, `align-content`, root overflow, scrolling, point hit testing,
  display-list translation, software rasterization, viewport projection,
  capture, and semantic/source-order consumers;
- auto-height columns, new intrinsic or percentage sizing, fractional lengths,
  negative margins, logical writing modes, baseline alignment, grid, normal
  flow auto-margin centering, and browser-wide Flexbox remain outside this
  bounded extension and retain their existing fallback behavior.

## Tradeoffs

- Resolving auto margins per formed line makes the common wrapped toolbar,
  card-grid, and fixed-height column patterns observable while keeping line
  formation and cross-line distribution owned by the already-tested 070-096
  paths. It does not claim a general multi-line Flexbox implementation.
- Auto margins remain zero during wrapping and provisional line sizing. This
  matches the important Flexbox ordering and prevents a margin declaration from
  changing which line receives an item, but it means line-size contributions
  stay limited to the existing numeric/content measurements.
- Cross-axis resolution runs after `align-content` has chosen each final line
  size. This preserves the existing line-distribution owner and makes
  `wrap-reverse` a physical reflection of the same resolved line geometry.
- Integer quotient-plus-prefix-remainder allocation is deterministic and
  preserves exact bounded totals, at the cost of subpixel and fractional
  distribution semantics that remain explicitly unsupported.
- Removing the blanket wrapped-auto fallback is safe only behind the existing
  eligibility gates. Any unsupported child or sizing shape must continue to
  use normal flow rather than partially entering the wrapped flex owner.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation gate will include:

- focused row and row-reverse `wrap`/`wrap-reverse` tests covering per-line
  main auto margins, cross auto margins, integer remainders, `justify-content`,
  `align-content`, and source/order preservation;
- focused column and column-reverse fixed-height `wrap`/`wrap-reverse` tests
  covering vertical main auto margins, horizontal cross auto margins,
  `row-gap`, `column-gap`, line distribution, and reverse physical placement;
- overflow and zero-free-space cases proving auto margins resolve to zero;
- explicit fallback tests for auto-height columns, unsupported direct children,
  and normal-flow auto margins;
- nested descendant geometry, display-list paint, software raster, viewport
  projection, capture, scroll, semantic, and point-hit consumers;
- `cargo fmt --all -- --check`, focused and full native integration tests,
  feature-enabled library tests with the documented explicit stack, strict
  all-feature and no-default-feature Clippy, warning-denied rustdoc, locked
  binaries, package/dependency gates, fuzz checking, documentation/release
  validators, and exact isolated-target cleanup.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
